use proc_macro::TokenStream;

#[proc_macro]
pub fn as_api(body: TokenStream) -> TokenStream {
    let decl: syn::ItemUse = syn::parse_str(&format!("{};", body))
        .inspect_err(|e| panic!("{e}"))
        .unwrap();
    quote::quote! {
        #decl
    }
    .into()
}

#[proc_macro_attribute]
pub fn api_endpoint(_attr: TokenStream, body: TokenStream) -> TokenStream {
    body
}
