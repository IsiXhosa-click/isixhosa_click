use crate::Config;
use anyhow::Context;
use fluent_templates::fluent_bundle::FluentResource;
use fluent_templates::{ArcLoader, Loader};
use std::path::PathBuf;
use std::sync::{Arc, Once};

pub type I18nInfo = isixhosa_common::i18n::I18nInfo<ArcLoader>;
pub type SiteContext = isixhosa_common::i18n::SiteContext<ArcLoader>;

pub use isixhosa_common::i18n::{ToTranslationKey, EN_ZA};

pub fn load(site: String, config: &Config) -> SiteContext {
    static ONLY_ONCE: Once = Once::new();

    if ONLY_ONCE.is_completed() {
        panic!("Can only load i18n once or else we will leak memory!")
    }

    ONLY_ONCE.call_once(|| {});

    let base: PathBuf = ["translations", "locales"].iter().collect();

    let mut site_specific_shared: PathBuf = ["translations", "site-specific"].iter().collect();
    site_specific_shared.push(&site);
    site_specific_shared.push("shared.ftl");

    let loader = ArcLoader::builder(&base, EN_ZA)
        .shared_resources(Some(&[
            ["translations", "locales", "shared.ftl"].iter().collect(),
            site_specific_shared,
        ]))
        .customize(move |bundle| {
            let mut site_ftl_path: PathBuf = ["translations", "site-specific"].iter().collect();
            site_ftl_path.push(&site);
            site_ftl_path.push(bundle.locales[0].to_string());
            site_ftl_path.push("main.ftl");

            if !site_ftl_path.is_file() {
                tracing::info!(
                    "Falling back to en-ZA site-specific file for {site} in {}",
                    bundle.locales[0]
                );
                site_ftl_path = ["translations", "site-specific", &site, "en-ZA", "main.ftl"]
                    .iter()
                    .collect();
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
        .expect("Error loading fluent resources");

    // Leaking is OK since this function should only be called once
    let supported: Vec<&'static str> = loader
        .locales()
        .map(|locale| &*locale.to_string().leak())
        .chain(Some("en"))
        .collect();
    let supported = &*supported.leak();

    SiteContext {
        site_i18n: loader,
        supported_langs: supported,
        host: config.host.clone(),
    }
}

/// Like From<T> but provides i18n context
pub trait FromWithI18n<T> {
    fn from_with_i18n(original: T, i18n: &I18nInfo) -> Self;
}

impl<T> FromWithI18n<T> for T {
    fn from_with_i18n(original: T, _i18n: &I18nInfo) -> T {
        original
    }
}
