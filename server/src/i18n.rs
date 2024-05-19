use anyhow::Context;
use fluent_templates::fluent_bundle::{FluentResource, FluentValue};
use fluent_templates::{ArcLoader, Loader};
use ordered_float::OrderedFloat;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::{Arc, Once};
use unic_langid::{langid, LanguageIdentifier};

#[derive(Clone)]
pub struct SiteContext {
    site_i18n: Arc<ArcLoader>,
    supported_langs: &'static [&'static str],
}

pub const EN_ZA: LanguageIdentifier = langid!("en-ZA");

pub fn load(site: String) -> SiteContext {
    static ONLY_ONCE: Once = Once::new();

    if ONLY_ONCE.is_completed() {
        panic!("Can only load i18n once or else we will leak memory!")
    }

    ONLY_ONCE.call_once(|| {});

    let base: PathBuf = ["translations", "locales"].iter().collect();

    let loader = ArcLoader::builder(&base, EN_ZA)
        .shared_resources(Some(&[["translations", "shared.ftl"].iter().collect()]))
        .customize(move |bundle| {
            let mut site_ftl_path: PathBuf = ["translations", "site-specific"].iter().collect();
            site_ftl_path.push(&site);
            site_ftl_path.push(bundle.locales[0].to_string());
            site_ftl_path.push("main.ftl");

            if !site_ftl_path.is_file() {
                return;
            }

            let site_ftl = std::fs::read_to_string(&site_ftl_path)
                .with_context(move || {
                    format!(
                        "Failed to read site-specific fluent file locale at {}",
                        site_ftl_path.display()
                    )
                })
                .unwrap();
            let site_resource = FluentResource::try_new(site_ftl)
                .expect("Couldn't parse site-specific fluent file");
            bundle.add_resource(Arc::new(site_resource)).unwrap();
        })
        .build()
        .expect("Couldn't load fluent translations");

    // Leaking is OK since this function should only be called once
    let supported: Vec<&'static str> = loader
        .locales()
        .map(|locale| &*locale.to_string().leak())
        .collect();
    let supported = &*supported.leak();

    SiteContext {
        site_i18n: Arc::new(loader),
        supported_langs: supported,
    }
}

#[derive(Clone)]
pub struct I18nInfo {
    pub user_language: LanguageIdentifier,
    pub ctx: SiteContext,
}

impl I18nInfo {
    pub fn parse_header(ctx: SiteContext, accept: Option<String>) -> I18nInfo {
        let all = accept_language::intersection_with_quality(
            accept.as_deref().unwrap_or("en-ZA"),
            ctx.supported_langs,
        );

        let best_lang = all
            .iter()
            .max_by_key(|(_lang, quality)| OrderedFloat(-quality))
            .map(|(lang, _quality)| lang.as_str())
            .unwrap_or("en-ZA");

        let user_language = best_lang.parse().unwrap_or(EN_ZA);

        I18nInfo { user_language, ctx }
    }
}

impl Debug for I18nInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("I18nInfo")
            .field("user_language", &self.user_language)
            .finish()
    }
}

macro_rules! args {
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

pub(crate) use args;

pub fn translate(
    key: &str,
    i18n: &I18nInfo,
    args: &HashMap<String, FluentValue<'static>>,
) -> String {
    i18n.ctx
        .site_i18n
        .lookup_with_args(&i18n.user_language, key, args)
}
