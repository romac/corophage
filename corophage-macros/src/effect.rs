use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::visit_mut::{self, VisitMut};
use syn::{GenericParam, ItemStruct, Lifetime, Result, Type, parse2};

use crate::crate_path;

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let mut resume_type: Type = parse2(attr)?;
    let item_struct: ItemStruct = parse2(item)?;

    let name = &item_struct.ident;

    let mut used_lifetimes: HashSet<String> = item_struct
        .generics
        .params
        .iter()
        .filter_map(|parameter| match parameter {
            GenericParam::Lifetime(parameter) => Some(parameter.lifetime.ident.to_string()),
            _ => None,
        })
        .collect();
    let resume_lifetime = fresh_lifetime("__corophage_resume", &mut used_lifetimes);
    let longer_lifetime = fresh_lifetime("__corophage_longer", &mut used_lifetimes);
    let shorter_lifetime = fresh_lifetime("__corophage_shorter", &mut used_lifetimes);

    ResumeLifetimeRewriter {
        replacement: &resume_lifetime,
    }
    .visit_type_mut(&mut resume_type);

    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();
    let corophage = crate_path::corophage()?;

    Ok(quote! {
        #item_struct

        impl #impl_generics #corophage::Effect for #name #ty_generics #where_clause {
            type Resume<#resume_lifetime> = #resume_type;

            #[inline]
            fn shorten_resume<#longer_lifetime: #shorter_lifetime, #shorter_lifetime>(
                resume: <Self as #corophage::Effect>::Resume<#longer_lifetime>,
            ) -> <Self as #corophage::Effect>::Resume<#shorter_lifetime> {
                resume
            }
        }
    })
}

fn fresh_lifetime(base: &str, used: &mut HashSet<String>) -> Lifetime {
    let mut suffix = None;

    loop {
        let name = match suffix {
            None => base.to_owned(),
            Some(suffix) => format!("{base}_{suffix}"),
        };

        if used.insert(name.clone()) {
            return Lifetime::new(&format!("'{name}"), proc_macro2::Span::mixed_site());
        }

        suffix = Some(suffix.map_or(2, |suffix| suffix + 1));
    }
}

struct ResumeLifetimeRewriter<'a> {
    replacement: &'a Lifetime,
}

impl VisitMut for ResumeLifetimeRewriter<'_> {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "r" {
            *lifetime = self.replacement.clone();
        } else {
            visit_mut::visit_lifetime_mut(self, lifetime);
        }
    }
}
