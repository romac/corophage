use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Error, Ident, Result};

pub fn corophage() -> Result<TokenStream> {
    match crate_name("corophage") {
        Ok(FoundCrate::Itself) => Ok(quote!(::corophage)),
        Ok(FoundCrate::Name(name)) => {
            let name = Ident::new(&name, Span::call_site());
            Ok(quote!(::#name))
        }
        Err(error) => Err(Error::new(
            Span::call_site(),
            format!("failed to locate the `corophage` crate: {error}"),
        )),
    }
}
