use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::fs::langid;
use fluent_templates::{ArcLoader, LanguageIdentifier, Loader};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

pub const EN_ZA: LanguageIdentifier = langid!("en-ZA");

pub struct SiteContext {
    pub site_i18n: ArcLoader,
    pub supported_langs: &'static [&'static str],
    pub host: String,
}

#[macro_export]
macro_rules! i18n_args {
    ($($arg:expr => $val:expr),*) => {
        {
            #[allow(unused_imports)]
            use ::fluent_templates::fluent_bundle::FluentValue;

            #[allow(unused_mut)]
            let mut hashmap = ::std::collections::HashMap::new();
            $(
                let val: FluentValue = $val.into();
                let val = match val {
                    FluentValue::String(s) => FluentValue::String(
                        std::borrow::Cow::Owned(
                            ::askama::MarkupDisplay::new_unsafe(s, ::askama::Html).to_string()
                        )
                    ),
                    v => v,
                };

                hashmap.insert($arg.to_string(), val);
            )*

            hashmap
        }
    };
}

#[derive(Clone)]
pub struct I18nInfo {
    pub user_language: LanguageIdentifier,
    pub ctx: Arc<SiteContext>,
}

impl Debug for I18nInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18nInfo")
            .field("user_language", &self.user_language)
            .finish()
    }
}

#[derive(Clone)]
pub struct TranslationKey<'a>(pub Cow<'a, str>);

impl TranslationKey<'_> {
    pub fn new(key: &str) -> TranslationKey<'_> {
        TranslationKey(Cow::Borrowed(key))
    }
}

pub trait ToTranslationKey {
    fn translation_key(&self) -> TranslationKey<'_>;
}

impl<T: ToTranslationKey> ToTranslationKey for &T {
    fn translation_key(&self) -> TranslationKey<'_> {
        T::translation_key(self)
    }
}

impl ToTranslationKey for &str {
    fn translation_key(&self) -> TranslationKey {
        TranslationKey(Cow::Borrowed(self))
    }
}

pub fn translate(
    key: &TranslationKey,
    i18n: &I18nInfo,
    args: &HashMap<String, FluentValue<'static>>,
) -> String {
    i18n.ctx
        .site_i18n
        .lookup_with_args(&i18n.user_language, key.0.as_ref(), args)
}
