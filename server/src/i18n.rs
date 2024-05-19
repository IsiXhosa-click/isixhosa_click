use anyhow::Context;
use fluent_templates::fluent_bundle::{FluentResource, FluentValue};
use fluent_templates::{ArcLoader, Loader};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;
use unic_langid::{langid, LanguageIdentifier};

#[derive(Clone)]
pub struct SiteContext {
    site_i18n: Arc<ArcLoader>,
}

pub const EN_ZA: &LanguageIdentifier = &langid!("en-ZA");

pub fn load(site: String) -> SiteContext {
    let base: PathBuf = ["translations", "locales"].iter().collect();
    let loader = ArcLoader::builder(&base, EN_ZA.clone())
        .shared_resources(Some(&[["translations", "shared.ftl"].iter().collect()]))
        .customize(move |bundle| {
            let mut site_ftl_path: PathBuf = ["translations", "site-specific"].iter().collect();
            site_ftl_path.push(&site);
            site_ftl_path.push(bundle.locales[0].to_string());
            site_ftl_path.push("main.ftl");
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

    SiteContext {
        site_i18n: Arc::new(loader),
    }
}

#[derive(Clone)]
pub struct I18nInfo {
    pub user_language: &'static LanguageIdentifier,
    pub ctx: SiteContext,
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
        .lookup_with_args(i18n.user_language, key, args)
}
