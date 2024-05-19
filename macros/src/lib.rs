use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(I18nTemplate)]
pub fn derive_i18n_template(input: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(input).unwrap();

    let name = &ast.ident;
    let gen = quote! {
        impl #name {
            fn translate(&self, key: &str) -> askama::MarkupDisplay<askama::Html, String> {
                askama::MarkupDisplay::new_safe(
                    crate::i18n::translate(key, &self.i18n_info, &HashMap::new()),
                    askama::Html
                )
            }

            fn t(&self, key: &str) -> askama::MarkupDisplay<askama::Html, String> {
                self.translate(key)
            }

            fn translate_with_arg(
                &self,
                key: &str,
                args: &HashMap<String, fluent_templates::fluent_bundle::FluentValue<'static>>
            ) -> String {
                crate::i18n::translate(key, &self.i18n_info, args)
            }

            fn t_with(
                &self,
                key: &str,
                args: &HashMap<String, fluent_templates::fluent_bundle::FluentValue<'static>>
            ) -> String {
                self.translate_with_arg(key, args)
            }
        }
    };
    gen.into()
}
