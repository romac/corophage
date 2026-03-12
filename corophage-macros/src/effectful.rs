use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{GenericParam, Ident, ItemFn, Lifetime, LifetimeParam, Result, ReturnType, Token, Type};

struct EffectfulArgs {
    lifetime: Option<Lifetime>,
    effects: Vec<Type>,
    send: bool,
}

impl Parse for EffectfulArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut lifetime = None;
        let mut effects = Vec::new();
        let mut send = false;

        if input.is_empty() {
            return Ok(EffectfulArgs {
                lifetime,
                effects,
                send,
            });
        }

        // Check if the first argument is a lifetime
        if input.peek(Lifetime) && (input.peek2(Token![,]) || input.is_empty()) {
            lifetime = Some(input.parse()?);
            if input.peek(Token![,]) {
                let _: Token![,] = input.parse()?;
            }
        }

        // Parse remaining as comma-separated types or `send` keyword
        let remaining: Punctuated<EffectArg, Token![,]> = Punctuated::parse_terminated(input)?;

        for arg in remaining {
            match arg {
                EffectArg::Send => send = true,
                EffectArg::Effect(ty) => effects.push(*ty),
            }
        }

        Ok(EffectfulArgs {
            lifetime,
            effects,
            send,
        })
    }
}

enum EffectArg {
    Send,
    Effect(Box<Type>),
}

impl Parse for EffectArg {
    fn parse(input: ParseStream) -> Result<Self> {
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

    if func.sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            func.sig.asyncness,
            "#[effectful] cannot be applied to async functions; \
             the macro already generates the async machinery internally",
        ));
    }

    let effects = &args.effects;

    // Determine the effect lifetime
    let eff_lifetime = determine_lifetime(&func, &args)?;

    // Remove the return type from the signature, we'll wrap it
    let return_type = match &func.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    let effects_type = quote! { ::corophage::Effects![#(#effects),*] };

    // Build the new return type
    let new_return_type = if args.send {
        quote! { ::corophage::Effectful<#eff_lifetime, #effects_type, #return_type, ::corophage::Sendable> }
    } else {
        quote! { ::corophage::Effectful<#eff_lifetime, #effects_type, #return_type> }
    };

    // Ensure the effect lifetime is in the generics
    ensure_lifetime_in_generics(&mut func, &eff_lifetime);

    // Update return type
    func.sig.output = ReturnType::Type(
        syn::token::RArrow::default(),
        Box::new(syn::parse2(new_return_type)?),
    );

    // Extract the original body
    let body = &func.block;

    // Build the new body with local macro_rules! for yield_!
    let program_constructor = if args.send {
        quote! { ::corophage::Program::new_send }
    } else {
        quote! { ::corophage::Program::new }
    };

    let new_body: syn::Block = syn::parse2(quote! {
        {
            #program_constructor(move |__y: ::corophage::Yielder<'_, #effects_type>| async move {
                #[allow(unused_macros)]
                macro_rules! yield_ {
                    ($eff:expr) => {
                        __y.yield_($eff).await
                    }
                }

                #[allow(unused_macros)]
                macro_rules! invoke {
                    ($prog:expr) => {
                        __y.invoke($prog).await
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
    // If explicitly provided, use it
    if let Some(lt) = &args.lifetime {
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
