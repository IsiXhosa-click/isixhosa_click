use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(I18nTemplate)]
pub fn derive_i18n_template(input: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(input).unwrap();

    let name = &ast.ident;
    let generics = &ast.generics;
    let where_clause = &generics.where_clause;

    let gen = quote! {
        impl #generics #name #generics #where_clause {
            fn host(&self) -> &str {
                &self.i18n_info.ctx.host
            }

            fn lang(&self) -> &fluent_templates::LanguageIdentifier {
                &self.i18n_info.user_language
            }

            fn translate<K: crate::i18n::ToTranslationKey>(&self, key: K) -> String {
                self.i18n_info.translate(&crate::i18n::ToTranslationKey::translation_key(&key))
            }

            fn t<K: crate::i18n::ToTranslationKey>(&self, key: K) -> String {
                self.translate(key)
            }

            fn translate_with_arg<K: crate::i18n::ToTranslationKey>(
                &self,
                key: K,
                args: &std::collections::HashMap<String, fluent_templates::fluent_bundle::FluentValue<'static>>
            ) -> String {
                self.i18n_info.translate_with(
                    &crate::i18n::ToTranslationKey::translation_key(&key),
                    args
                )
            }

            fn t_with<K: crate::i18n::ToTranslationKey>(
                &self,
                key: K,
                args: &std::collections::HashMap<String, fluent_templates::fluent_bundle::FluentValue<'static>>
            ) -> String {
                self.translate_with_arg(key, args)
            }
        }
    };
    gen.into()
}
