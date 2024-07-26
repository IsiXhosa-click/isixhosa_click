use anyhow::{anyhow, bail, Context, Result};
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

async fn get_translation(lang: &str) -> Result<String> {
    let url = format!("/translations/{lang}/main.ftl");
    let res = Request::get(&url).send().await?;

    if res.status() != 200 {
        bail!("Not 200 OK for fetching resource at {url}!");
    }

    res.text()
        .await
        .with_context(|| format!("Failed to fetch resource at {url}"))
}

pub async fn load() -> Result<&'static StaticLoader> {
    log::debug!("Fetching available locales");
    let supported_langs: Vec<LanguageIdentifier> =
        Request::get("/translations").send().await?.json().await?;

    log::debug!("Fetching site locale files");
    let mut site_files = HashMap::new();
    for lang in supported_langs {
        let raw = get_translation(&lang.to_string()).await;

        let raw = match raw {
            Ok(r) => r,
            Err(_) => {
                log::debug!("Falling back to en-ZA for {}", lang);
                get_translation("en-ZA")
                    .await
                    .expect("Failed to fetch en-ZA site files")
            },
        };

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
            .context("Invalid fluent resource")?;

        site_files.insert(lang, resource);
    }

    I18N_SITE_FILES
        .set(site_files)
        .expect("Tried to initialize i18n twice!");
    Ok(&LOCALES)
}
