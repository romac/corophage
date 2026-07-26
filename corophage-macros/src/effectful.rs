use proc_macro2::{Punct, Spacing, TokenStream};
use quote::{ToTokens, TokenStreamExt, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
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
}

impl Parse for EffectfulArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut lifetime = None;
        let mut effects = Vec::new();
        let mut spread = None;
        let mut send = false;

        if input.is_empty() {
            return Ok(EffectfulArgs {
                lifetime,
                effects,
                spread,
                send,
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
        })
    }
}

enum EffectArg {
    Send,
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
    let new_return_type = if args.send {
        quote! { #corophage::Effectful<#eff_lifetime, #effects_type, #return_type, #corophage::Sendable> }
    } else {
        quote! { #corophage::Effectful<#eff_lifetime, #effects_type, #return_type> }
    };

    // Ensure captured arguments can live for the lifetime of the program.
    ensure_lifetime_in_generics(&mut func, &eff_lifetime);
    bind_inputs_to_effect_lifetime(&mut func, &eff_lifetime);

    // Update return type
    func.sig.output = ReturnType::Type(
        syn::token::RArrow::default(),
        Box::new(syn::parse2(new_return_type)?),
    );

    // Extract the original body
    let body = &func.block;

    // Build the new body with local macro_rules! for yield_!
    let program_constructor = if args.send {
        quote! { #corophage::Program::new_send }
    } else {
        quote! { #corophage::Program::new }
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
            })
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
                let mut binder = InputLifetimeBinder::new(effect_lifetime);
                binder.visit_type_mut(&mut argument.ty);
                predicates.extend(binder.reference_predicates);

                if !binder.contains_impl_trait {
                    let ty = &argument.ty;
                    predicates.push(parse_quote!(#ty: #effect_lifetime));
                }
            }
        }
    }

    func.sig
        .generics
        .make_where_clause()
        .predicates
        .extend(predicates);
}

struct InputLifetimeBinder<'a> {
    effect_lifetime: &'a Lifetime,
    contains_impl_trait: bool,
    reference_predicates: Vec<WherePredicate>,
}

impl<'a> InputLifetimeBinder<'a> {
    fn new(effect_lifetime: &'a Lifetime) -> Self {
        Self {
            effect_lifetime,
            contains_impl_trait: false,
            reference_predicates: Vec::new(),
        }
    }
}

impl VisitMut for InputLifetimeBinder<'_> {
    fn visit_type_reference_mut(&mut self, reference: &mut syn::TypeReference) {
        match &mut reference.lifetime {
            Some(lifetime) if lifetime.ident != "_" => {
                if lifetime != self.effect_lifetime && lifetime.ident != "static" {
                    let lifetime = lifetime.clone();
                    let effect_lifetime = self.effect_lifetime;
                    self.reference_predicates
                        .push(parse_quote!(#lifetime: #effect_lifetime));
                }
            }
            lifetime => *lifetime = Some(self.effect_lifetime.clone()),
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
