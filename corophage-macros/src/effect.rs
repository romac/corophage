use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::visit::Visit;
use syn::{GenericParam, Ident, ItemStruct, Result, Type, parse2};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let resume_type: Type = parse2(attr)?;
    let item_struct: ItemStruct = parse2(item)?;

    let name = &item_struct.ident;

    // Collect type parameter names from the struct
    let type_param_names: HashSet<Ident> = item_struct
        .generics
        .params
        .iter()
        .filter_map(|p| match p {
            GenericParam::Type(tp) => Some(tp.ident.clone()),
            _ => None,
        })
        .collect();

    // Find which type parameters appear in the resume type
    let mut visitor = TypeParamVisitor {
        type_params: &type_param_names,
        found: HashSet::new(),
    };
    visitor.visit_type(&resume_type);
    let used_in_resume = visitor.found;

    // Clone generics and add Send + Sync bounds for type params used in the resume type
    let mut impl_generics_def = item_struct.generics.clone();
    for param in &mut impl_generics_def.params {
        if let GenericParam::Type(tp) = param {
            if used_in_resume.contains(&tp.ident) {
                tp.bounds.push(syn::parse_quote!(Send));
                tp.bounds.push(syn::parse_quote!(Sync));
            }
        }
    }

    let (impl_generics, _, _) = impl_generics_def.split_for_impl();
    let (_, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    Ok(quote! {
        #item_struct

        impl #impl_generics ::corophage::Effect for #name #ty_generics #where_clause {
            type Resume<'r> = #resume_type;
        }
    })
}

/// Visitor that finds which type parameter identifiers appear in a type.
struct TypeParamVisitor<'a> {
    type_params: &'a HashSet<Ident>,
    found: HashSet<Ident>,
}

impl<'a> Visit<'a> for TypeParamVisitor<'_> {
    fn visit_path(&mut self, path: &'a syn::Path) {
        // A bare identifier like `S` appears as a single-segment path
        if path.leading_colon.is_none() && path.segments.len() == 1 {
            let seg = &path.segments[0];
            if self.type_params.contains(&seg.ident) {
                self.found.insert(seg.ident.clone());
            }
        }
        syn::visit::visit_path(self, path);
    }
}
