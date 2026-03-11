use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemStruct, Result, Type, parse2};

pub fn expand(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let resume_type: Type = parse2(attr)?;
    let item_struct: ItemStruct = parse2(item)?;

    let name = &item_struct.ident;

    let (impl_generics, ty_generics, where_clause) = item_struct.generics.split_for_impl();

    Ok(quote! {
        #item_struct

        impl #impl_generics ::corophage::Effect for #name #ty_generics #where_clause {
            type Resume<'r> = #resume_type;
        }
    })
}
