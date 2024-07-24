use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::fs::langid;
use fluent_templates::{LanguageIdentifier, Loader};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

pub const EN_ZA: LanguageIdentifier = langid!("en-ZA");

pub struct SiteContext<L> {
    pub site_i18n: L,
    pub supported_langs: &'static [&'static str],
    pub host: String,
}

#[derive(Ord, PartialOrd, Eq, PartialEq)]
pub struct Language {
    pub name: String,
    pub flag: String,
    pub id: LanguageIdentifier,
}

impl<L: Loader> SiteContext<L> {
    pub fn supported_languages(&self) -> Vec<Language> {
        let mut vec: Vec<Language> = self
            .site_i18n
            .locales()
            .map(|id| Language {
                name: self.site_i18n.lookup(id, "ui-language"),
                flag: self.site_i18n.lookup(id, "ui-language.flag"),
                id: id.clone(),
            })
            .collect();
        vec.sort();
        vec
    }
}

#[macro_export]
macro_rules! i18n_args {
    ($($arg:expr => $val:expr),*$(,)?) => {
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

#[macro_export]
macro_rules! i18n_args_unescaped {
    ($($arg:expr => $val:expr),*$(,)?) => {
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
                            ::askama::MarkupDisplay::new_safe(s, ::askama::Html).to_string()
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

pub struct I18nInfo<L> {
    pub user_language: LanguageIdentifier,
    pub ctx: Arc<SiteContext<L>>,
}

impl<L: Loader + 'static> I18nInfo<L> {
    pub fn translate_with(
        &self,
        key: &TranslationKey,
        args: &HashMap<String, FluentValue<'static>>,
    ) -> String {
        self.ctx
            .site_i18n
            .lookup_with_args(&self.user_language, key.0.as_ref(), args)
    }

    pub fn t_with(
        &self,
        key: &TranslationKey,
        args: &HashMap<String, FluentValue<'static>>,
    ) -> String {
        self.translate_with(key, args)
    }

    pub fn translate(&self, key: &TranslationKey) -> String {
        self.translate_with(key, &HashMap::new())
    }

    pub fn t(&self, key: &TranslationKey) -> String {
        self.translate(key)
    }
}

impl<L> PartialEq<I18nInfo<L>> for I18nInfo<L> {
    fn eq(&self, other: &I18nInfo<L>) -> bool {
        self.user_language == other.user_language
    }
}

impl<L> Clone for I18nInfo<L> {
    fn clone(&self) -> Self {
        I18nInfo {
            user_language: self.user_language.clone(),
            ctx: self.ctx.clone(),
        }
    }
}

impl<L: Loader + 'static> I18nInfo<L> {
    pub fn js_translations(&self) -> HashMap<&'static str, String> {
        // TODO I don't like this hack :(
        [
            "search.no-results",
            "plurality.plural",
            "informal.in-word-result",
            "inchoative.in-word-result",
            "transitive.in-word-result",
            "intransitive.in-word-result",
            "ambitransitive.in-word-result",
            "noun-class.in-word-result",
            "verb",
            "noun",
            "adjective",
            "adverb",
            "relative",
            "interjection",
            "conjunction",
            "preposition",
            "ideophone",
            "bound_morpheme",
            "linked-words.choose",
            "linked-words.search",
            "linked-words.plurality",
            "linked-words.alternate",
            "linked-words.antonym",
            "linked-words.related",
            "linked-words.confusable",
            "examples.source",
            "examples.target",
            "delete",
        ]
        .into_iter()
        .map(|key| (key, self.ctx.site_i18n.lookup(&self.user_language, key)))
        .collect()
    }
}

impl<L: Loader + 'static> Debug for I18nInfo<L> {
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
