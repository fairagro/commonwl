use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, parse_macro_input};

#[proc_macro_derive(Identifiable)]
/// Derive Macro for `Identifiable`
/// # Panics
/// - If a non struct type is given
/// - Struct does not have id field
pub fn derive_identifiable(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    // Only allow structs
    let fields = match input.data {
        Data::Struct(data) => data.fields,
        _ => panic!("Identifiable can only be derived for structs"),
    };

    // Look for a field named `id: Option<String>`
    let has_id = fields
        .iter()
        .any(|f| f.ident.as_ref().is_some_and(|i| i == "id"));

    assert!(has_id, "Identifiable requires a field named `id`");

    let expanded = quote! {
        impl Identifiable for #name {
            fn get_id(&self) -> Option<&String> {
                self.id.as_ref()
            }

            fn set_id(&mut self, value: &str) {
                self.id = Some(value.to_string());
            }
        }
    };

    TokenStream::from(expanded)
}
