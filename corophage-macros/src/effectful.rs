use std::collections::HashSet;

use proc_macro2::{Punct, Spacing, TokenStream};
use quote::{ToTokens, TokenStreamExt, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::visit_mut::{self, VisitMut};
use syn::{
    GenericParam, Ident, ItemFn, Lifetime, LifetimeParam, Result, ReturnType, Token, Type,
    TypeParamBound, WherePredicate, parse_quote,
};

use crate::crate_path;

struct EffectfulArgs {
    lifetime: Option<Lifetime>,
    effects: Vec<Type>,
    spread: Option<Type>,
    send: bool,
    boxed: bool,
}

impl Parse for EffectfulArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut lifetime = None;
        let mut effects = Vec::new();
        let mut spread = None;
        let mut send = false;
        let mut boxed = false;

        if input.is_empty() {
            return Ok(EffectfulArgs {
                lifetime,
                effects,
                spread,
                send,
                boxed,
            });
        }

        // Check if the first argument is a lifetime. A comma is only required
        // when more attribute arguments follow it.
        if input.peek(Lifetime) {
            lifetime = Some(input.parse()?);
            if !input.is_empty() {
                let _: Token![,] = input.parse()?;
            }
        }

        // Parse remaining as comma-separated types, `send` keyword, or `...Type` spread
        let remaining: Punctuated<EffectArg, Token![,]> = Punctuated::parse_terminated(input)?;

        for arg in remaining {
            match arg {
                EffectArg::Send => send = true,
                EffectArg::Boxed => boxed = true,
                EffectArg::Effect(ty) => {
                    if spread.is_some() {
                        return Err(syn::Error::new_spanned(
                            ty,
                            "`...Spread` must be the last effect argument \
                             (before `send`); inline effects cannot follow a spread",
                        ));
                    }
                    effects.push(*ty);
                }
                EffectArg::Spread(ty) => {
                    if spread.is_some() {
                        return Err(syn::Error::new_spanned(
                            ty,
                            "only one `...Spread` is allowed per #[effectful] attribute",
                        ));
                    }
                    spread = Some(*ty);
                }
            }
        }

        Ok(EffectfulArgs {
            lifetime,
            effects,
            spread,
            send,
            boxed,
        })
    }
}

enum EffectArg {
    Send,
    Boxed,
    Effect(Box<Type>),
    Spread(Box<Type>),
}

/// Wrapper that emits `...Type` tokens for use in `Coprod!(... Type)`.
struct SpreadType<'a>(&'a Type);

impl ToTokens for SpreadType<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(Punct::new('.', Spacing::Joint));
        tokens.append(Punct::new('.', Spacing::Joint));
        tokens.append(Punct::new('.', Spacing::Alone));
        self.0.to_tokens(tokens);
    }
}

/// Check if the next tokens are `...` (three consecutive dots).
fn peek_spread(input: ParseStream) -> bool {
    let fork = input.fork();
    for i in 0..3 {
        match fork.step(|cursor| match cursor.punct() {
            Some((punct, rest))
                if punct.as_char() == '.' && (i == 2 || punct.spacing() == Spacing::Joint) =>
            {
                Ok(((), rest))
            }
            _ => Err(cursor.error("expected `.`")),
        }) {
            Ok(()) => {}
            Err(_) => return false,
        }
    }
    true
}

/// Consume `...` (three dots) from the input stream.
fn parse_spread_dots(input: ParseStream) -> Result<()> {
    for _ in 0..3 {
        input.step(|cursor| match cursor.punct() {
            Some((punct, rest)) if punct.as_char() == '.' => Ok(((), rest)),
            _ => Err(cursor.error("expected `.`")),
        })?;
    }
    Ok(())
}

impl Parse for EffectArg {
    fn parse(input: ParseStream) -> Result<Self> {
        // Check for `...Type` spread syntax
        if peek_spread(input) {
            parse_spread_dots(input)?;
            let ty: Type = input.parse()?;
            return Ok(EffectArg::Spread(Box::new(ty)));
        }

        // Check for `send` keyword
        if input.peek(Ident) {
            let fork = input.fork();
            let ident: Ident = fork.parse()?;
            if ident == "send" && (fork.is_empty() || fork.peek(Token![,])) {
                // Consume from the actual stream
                let _: Ident = input.parse()?;
                return Ok(EffectArg::Send);
            }
            if ident == "boxed" && (fork.is_empty() || fork.peek(Token![,])) {
                let _: Ident = input.parse()?;
                return Ok(EffectArg::Boxed);
            }
        }
        Ok(EffectArg::Effect(Box::new(input.parse()?)))
    }
}

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args: EffectfulArgs = syn::parse2(attr)?;
    let mut func: ItemFn = syn::parse2(item)?;
    let corophage = crate_path::corophage()?;

    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            func.sig.asyncness,
            "#[effectful] cannot be applied to async functions; \
             the macro already generates the async machinery internally",
        ));
    }

    let effects = &args.effects;
    let spread = &args.spread;

    // Determine the effect lifetime
    let eff_lifetime = determine_lifetime(&func, &args)?;

    // Remove the return type from the signature, we'll wrap it
    let return_type = match &func.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    let effects_type = match (effects.as_slice(), spread) {
        ([], None) => quote! { #corophage::Effects![] },
        (effects, None) => quote! { #corophage::Effects![#(#effects),*] },
        (effects, Some(spread)) => {
            let spread = SpreadType(spread);
            quote! { #corophage::Effects![#(#effects,)* #spread] }
        }
    };

    // Build the new return type
    let new_return_type = if args.boxed && args.send {
        quote! { #corophage::Effectful<#eff_lifetime, #effects_type, #return_type, #corophage::Sendable> }
    } else if args.boxed {
        quote! { #corophage::Effectful<#eff_lifetime, #effects_type, #return_type> }
    } else {
        let send = args.send.then(|| quote! { + ::core::marker::Send });
        quote! { #corophage::Program<#eff_lifetime, #effects_type, impl ::core::future::Future<Output = #return_type> + #eff_lifetime #send> }
    };

    // Ensure captured arguments can live for the lifetime of the program.
    // Explicit names refer to an existing declaration, possibly on an enclosing
    // impl or trait that this attribute macro cannot see.
    if args.lifetime.as_ref().is_none_or(|lt| lt.ident == "_") {
        ensure_lifetime_in_generics(&mut func, &eff_lifetime);
    }
    // Bind type parameters first so they see only the user's where clause.
    bind_type_parameters_to_effect_lifetime(&mut func, &eff_lifetime);
    bind_inputs_to_effect_lifetime(&mut func, &eff_lifetime);

    // Update return type
    func.sig.output = ReturnType::Type(
        syn::token::RArrow::default(),
        Box::new(syn::parse2(new_return_type)?),
    );

    // Extract the original body
    let body = &func.block;

    // Build the new body with local macro_rules! for yield_!
    let program_constructor = quote! { #corophage::Program::new };
    let boxing = if args.boxed && args.send {
        quote! { .boxed_send() }
    } else if args.boxed {
        quote! { .boxed() }
    } else {
        quote! {}
    };
    let yielder = Ident::new("__corophage_yielder", proc_macro2::Span::mixed_site());

    let new_body: syn::Block = syn::parse2(quote! {
        {
            #program_constructor(move |mut #yielder: #corophage::Yielder<'_, #effects_type>| async move {
                #[allow(unused_macros)]
                macro_rules! yield_ {
                    ($eff:expr) => {
                        #yielder.yield_($eff).await
                    }
                }

                #[allow(unused_macros)]
                macro_rules! invoke {
                    ($prog:expr) => {
                        #yielder.invoke($prog).await
                    }
                }

                #body
            }) #boxing
        }
    })?;

    *func.block = new_body;

    Ok(quote! { #func })
}

fn determine_lifetime(func: &ItemFn, args: &EffectfulArgs) -> Result<Lifetime> {
    // If explicitly provided, use it. `'_` requests the same inference as an
    // omitted lifetime rather than becoming an invalid generic parameter.
    if let Some(lt) = &args.lifetime
        && lt.ident != "_"
    {
        return Ok(lt.clone());
    }

    // Collect lifetime params from the function
    let lifetime_params: Vec<&LifetimeParam> = func
        .sig
        .generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Lifetime(lt) => Some(lt),
            _ => None,
        })
        .collect();

    match lifetime_params.len() {
        0 => {
            // No lifetimes on the function — generate '__eff
            Ok(Lifetime::new("'__eff", proc_macro2::Span::call_site()))
        }
        1 => {
            // Single lifetime — use it
            Ok(lifetime_params[0].lifetime.clone())
        }
        _ => {
            // Multiple lifetimes — error
            Err(syn::Error::new_spanned(
                &func.sig.generics,
                "function has multiple lifetime parameters; \
                 please specify which lifetime to use for effects \
                 as the first argument: #[effectful('a, Eff1, Eff2)]",
            ))
        }
    }
}

fn ensure_lifetime_in_generics(func: &mut ItemFn, lifetime: &Lifetime) {
    if lifetime.ident == "static" || lifetime.ident == "_" {
        return;
    }

    let already_exists = func.sig.generics.params.iter().any(|p| match p {
        GenericParam::Lifetime(lt) => lt.lifetime == *lifetime,
        _ => false,
    });

    if !already_exists {
        let lt_param = LifetimeParam::new(lifetime.clone());
        func.sig
            .generics
            .params
            .insert(0, GenericParam::Lifetime(lt_param));
    }
}

fn bind_inputs_to_effect_lifetime(func: &mut ItemFn, effect_lifetime: &Lifetime) {
    let mut predicates = Vec::new();
    let mut binder = InputLifetimeBinder::new(effect_lifetime);
    // Include higher-ranked lifetimes and bounds when avoiding name collisions.
    binder.visit_signature(&func.sig);

    for input in &mut func.sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                if let Some((_, lifetime)) = &mut receiver.reference {
                    match lifetime {
                        Some(lifetime) if lifetime.ident != "_" => {
                            if lifetime != effect_lifetime && lifetime.ident != "static" {
                                let lifetime = lifetime.clone();
                                predicates.push(parse_quote!(#lifetime: #effect_lifetime));
                            }
                        }
                        lifetime => *lifetime = Some(effect_lifetime.clone()),
                    }
                } else {
                    predicates.push(parse_quote!(Self: #effect_lifetime));
                }
            }
            syn::FnArg::Typed(argument) => {
                binder.contains_impl_trait = false;
                binder.visit_type_mut(&mut argument.ty);
                predicates.append(&mut binder.reference_predicates);

                // A bare type parameter already received this bound.
                if !binder.contains_impl_trait
                    && !is_type_parameter(&func.sig.generics, &argument.ty)
                {
                    let ty = &argument.ty;
                    predicates.push(parse_quote!(#ty: #effect_lifetime));
                }
            }
        }
    }

    for lifetime in binder.inferred_lifetimes {
        func.sig
            .generics
            .params
            .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
    }

    func.sig
        .generics
        .make_where_clause()
        .predicates
        .extend(predicates);
}

// Type parameters may appear only in effects or the return type. The program
// body captures every type parameter, so this bound adds no restriction.
// Add it where the parameter is already bounded, so Clippy's
// `multiple_bound_locations` lint does not fire on the expanded signature.
fn bind_type_parameters_to_effect_lifetime(func: &mut ItemFn, effect_lifetime: &Lifetime) {
    let generics = &mut func.sig.generics;
    for parameter in generics.params.iter_mut() {
        let GenericParam::Type(parameter) = parameter else {
            continue;
        };
        let bound = TypeParamBound::Lifetime(effect_lifetime.clone());
        let where_bounds = generics.where_clause.as_mut().and_then(|clause| {
            clause
                .predicates
                .iter_mut()
                .find_map(|predicate| match predicate {
                    WherePredicate::Type(predicate)
                        if predicate.lifetimes.is_none()
                            && is_named(&predicate.bounded_ty, &parameter.ident) =>
                    {
                        Some(&mut predicate.bounds)
                    }
                    _ => None,
                })
        });
        match where_bounds {
            Some(bounds) if parameter.bounds.is_empty() => bounds.push(bound),
            _ => parameter.bounds.push(bound),
        }
    }
}

fn is_type_parameter(generics: &syn::Generics, ty: &Type) -> bool {
    generics
        .type_params()
        .any(|parameter| is_named(ty, &parameter.ident))
}

fn is_named(ty: &Type, ident: &Ident) -> bool {
    matches!(ty, Type::Path(path) if path.qself.is_none() && path.path.is_ident(ident))
}

struct InputLifetimeBinder<'a> {
    effect_lifetime: &'a Lifetime,
    contains_impl_trait: bool,
    reference_predicates: Vec<WherePredicate>,
    used_lifetimes: HashSet<String>,
    inferred_lifetimes: Vec<Lifetime>,
}

impl<'a> InputLifetimeBinder<'a> {
    fn new(effect_lifetime: &'a Lifetime) -> Self {
        Self {
            effect_lifetime,
            contains_impl_trait: false,
            reference_predicates: Vec::new(),
            // The selected lifetime may appear only in the attribute when it
            // comes from an enclosing scope, so reserve it before visiting inputs.
            used_lifetimes: HashSet::from([effect_lifetime.ident.to_string()]),
            inferred_lifetimes: Vec::new(),
        }
    }

    fn fresh_input_lifetime(&mut self) -> Lifetime {
        let mut suffix = self.inferred_lifetimes.len();
        loop {
            let name = format!("__corophage_input_{suffix}");
            if self.used_lifetimes.insert(name.clone()) {
                let lifetime = Lifetime::new(&format!("'{name}"), proc_macro2::Span::mixed_site());
                self.inferred_lifetimes.push(lifetime.clone());
                return lifetime;
            }
            suffix += 1;
        }
    }
}

impl<'ast> Visit<'ast> for InputLifetimeBinder<'_> {
    fn visit_lifetime(&mut self, lifetime: &'ast Lifetime) {
        self.used_lifetimes.insert(lifetime.ident.to_string());
    }
}

impl VisitMut for InputLifetimeBinder<'_> {
    fn visit_generic_argument_mut(&mut self, argument: &mut syn::GenericArgument) {
        if let syn::GenericArgument::Lifetime(lifetime) = argument
            && lifetime.ident == "_"
        {
            // `'_` is invalid in the generated `ArgumentType: 'effect` bound.
            // Give each occurrence its own parameter so invariant wrappers do
            // not force their contents to share the program's lifetime.
            *lifetime = self.fresh_input_lifetime();
        } else {
            visit_mut::visit_generic_argument_mut(self, argument);
        }
    }

    fn visit_type_reference_mut(&mut self, reference: &mut syn::TypeReference) {
        let lifetime = match &reference.lifetime {
            Some(lifetime) if lifetime.ident != "_" => lifetime.clone(),
            _ => {
                // Nested references need distinct lifetimes: the outer borrow
                // may end while the data behind an inner reference stays live.
                let lifetime = self.fresh_input_lifetime();
                reference.lifetime = Some(lifetime.clone());
                lifetime
            }
        };
        if lifetime != *self.effect_lifetime && lifetime.ident != "static" {
            let effect_lifetime = self.effect_lifetime;
            self.reference_predicates
                .push(parse_quote!(#lifetime: #effect_lifetime));
        }

        visit_mut::visit_type_reference_mut(self, reference);
    }

    fn visit_type_impl_trait_mut(&mut self, impl_trait: &mut syn::TypeImplTrait) {
        self.contains_impl_trait = true;

        let effect_lifetime = self.effect_lifetime;
        if !impl_trait.bounds.iter().any(
            |bound| matches!(bound, TypeParamBound::Lifetime(lifetime) if lifetime == effect_lifetime),
        ) {
            impl_trait
                .bounds
                .push(TypeParamBound::Lifetime(effect_lifetime.clone()));
        }
    }

    fn visit_type_bare_fn_mut(&mut self, _bare_fn: &mut syn::TypeBareFn) {}

    fn visit_trait_bound_mut(&mut self, _trait_bound: &mut syn::TraitBound) {}
}
