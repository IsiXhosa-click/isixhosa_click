use anyhow::{anyhow, Context, Result};
use fluent_templates::fluent_bundle::FluentResource;
use fluent_templates::{static_loader, StaticLoader};
use gloo::net::http::Request;
use itertools::Itertools;
use std::collections::HashMap;
use std::sync::OnceLock;
use unic_langid::LanguageIdentifier;

static I18N_SITE_FILES: OnceLock<HashMap<LanguageIdentifier, FluentResource>> = OnceLock::new();

static_loader! {
    static LOCALES = {
        locales: "../../server/translations/locales",
        fallback_language: "en-ZA",
        core_locales: "../../server/translations/locales/shared.ftl",
        customise: |bundle| {
            bundle.add_resource(&I18N_SITE_FILES.get().unwrap()[&bundle.locales[0]]).unwrap();
        }
    };
}

pub async fn load() -> Result<&'static StaticLoader> {
    log::debug!("Fetching available locales");
    let supported_langs: Vec<LanguageIdentifier> =
        Request::get("/translations").send().await?.json().await?;

    log::debug!("Fetching site locale files");
    let mut site_files = HashMap::new();
    for lang in supported_langs {
        let url = format!("/translations/{lang}/main.ftl");
        let raw = Request::get(&url)
            .send()
            .await?
            .text()
            .await
            .with_context(|| format!("Failed to fetch resource at {url}"))?;

        let resource = FluentResource::try_new(raw)
            .map_err(|(_res, errs)| {
                let err = errs
                    .into_iter()
                    .enumerate()
                    .map(|(i, e)| (i + 1, e))
                    .map(|(err_no, e)| format!("{err_no}: {e}"))
                    .join("\n");

                anyhow!(err)
            })
            .with_context(|| format!("Invalid fluent resource at {url}"))?;

        site_files.insert(lang, resource);
    }

    I18N_SITE_FILES
        .set(site_files)
        .expect("Tried to initialize i18n twice!");
    Ok(&LOCALES)
}
