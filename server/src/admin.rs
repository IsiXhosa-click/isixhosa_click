use crate::auth::{with_administrator_auth, FullUser};
use crate::i18n::{I18nInfo, SiteContext};
use crate::{spawn_blocking_child, DebugBoxedExt};
use anyhow::{bail, Context, Result};
use askama::Template;
use futures::StreamExt;
use image::{DynamicImage, ImageFormat, ImageReader};
use isixhosa_click_macros::I18nTemplate;
use isixhosa_common::auth::Auth;
use isixhosa_common::database::{AdministratorAccessDb, DbBase};
use isixhosa_common::types::Dataset;
use std::io::{Cursor, Read};
use std::sync::Arc;
use warp::multipart::FormData;
use warp::Buf;
use warp::{Filter, Rejection, Reply};

pub fn admin(
    db: DbBase,
    site_ctx: Arc<SiteContext>,
) -> impl Filter<Error = Rejection, Extract = impl Reply> + Clone {
    let base = with_administrator_auth(db, site_ctx);

    let settings = warp::path::end()
        .and(base.clone())
        .and(warp::any().map(|| Ok(Action::None)))
        .and_then(reply_settings);

    let add_dataset_route = warp::path("add_dataset").and(warp::path::end());

    let add_dataset_form = add_dataset_route.and(warp::get()).and(base.clone()).map(
        |user: FullUser, i18n_info, _db| AddDataset {
            auth: user.into(),
            i18n_info,
            dataset: Default::default(),
        },
    );

    let add_dataset_submit = add_dataset_route
        .and(base.clone())
        .and(warp::post())
        .and(warp::multipart::form().max_length(Some(16 * 1024 * 1024)))
        .and_then(reply_add_dataset);

    let edit_dataset_form = warp::path!("dataset" / u64 / "edit")
        .and(base.clone())
        .and(warp::path::end())
        .and(warp::get())
        .and_then(reply_edit_dataset_form);

    let delete_dataset = warp::path!("dataset" / u64 / "delete")
        .and(base.clone())
        .and(warp::path::end())
        .and(warp::post())
        .and_then(reply_delete_dataset);

    warp::path!("admin" / "settings" / ..)
        .and(
            settings
                .or(add_dataset_form)
                .or(add_dataset_submit)
                .or(edit_dataset_form)
                .or(delete_dataset),
        )
        .debug_boxed()
}

enum Action {
    None,
    AddDataset,
    DeleteDataset,
}

async fn reply_settings(
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl AdministratorAccessDb,
    previous_success: Result<Action, Action>,
) -> Result<impl Reply, Rejection> {
    Ok(SiteSettings {
        auth: user.into(),
        i18n_info,
        datasets: spawn_blocking_child(move || Dataset::fetch_all(&db))
            .await
            .unwrap(),
        previous_success,
    })
}

async fn add_dataset_from_data(form: FormData, db: &impl AdministratorAccessDb) -> Result<()> {
    let (dataset, icon) = DatasetForm::try_from_multipart(form).await?;
    let icon_bytes = match icon {
        Some(icon) => {
            let mut vec = vec![];
            icon.write_to(&mut Cursor::new(&mut vec), ImageFormat::Png)?;
            Some(vec)
        }
        None => None,
    };

    let str_opt = |s: String| if s.is_empty() { None } else { Some(s) };

    let db_clone = db.clone();
    spawn_blocking_child(move || {
        Dataset::upsert(
            &db_clone,
            dataset.id,
            dataset.name,
            dataset.description,
            dataset.author,
            dataset.license,
            str_opt(dataset.institution),
            icon_bytes,
            str_opt(dataset.url),
        )
    })
    .await
    .context("Failed to join task")
    .and_then(|x| x)?;

    Ok(())
}

async fn reply_add_dataset(
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl AdministratorAccessDb,
    form: FormData,
) -> Result<impl Reply, Rejection> {
    let success = match add_dataset_from_data(form, &db).await {
        Ok(_) => Ok(Action::AddDataset),
        Err(error) => {
            tracing::error!(?error, "Failed to add dataset");
            Err(Action::AddDataset)
        }
    };

    reply_settings(user, i18n_info, db, success).await
}

async fn reply_delete_dataset(
    dataset_id: u64,
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl AdministratorAccessDb,
) -> Result<impl Reply, Rejection> {
    let db_clone = db.clone();
    let success = spawn_blocking_child(move || Dataset::delete_by_id(&db_clone, dataset_id))
        .await
        .unwrap();

    let success = if success {
        Ok(Action::DeleteDataset)
    } else {
        Err(Action::DeleteDataset)
    };

    reply_settings(user, i18n_info, db, success).await
}

async fn reply_edit_dataset_form(
    dataset_id: u64,
    user: FullUser,
    i18n_info: I18nInfo,
    db: impl AdministratorAccessDb,
) -> Result<impl Reply, Rejection> {
    let dataset = spawn_blocking_child(move || Dataset::fetch_by_id(&db, dataset_id))
        .await
        .unwrap()
        .ok_or(warp::reject::not_found())?
        .into();

    Ok(AddDataset {
        auth: user.into(),
        i18n_info,
        dataset,
    })
}

#[derive(Default, Debug)]
struct DatasetForm {
    pub id: Option<u64>,
    pub name: String,
    pub description: String,
    pub author: String,
    pub license: String,
    pub institution: String,
    pub url: String,
}

impl DatasetForm {
    async fn try_from_multipart(mut data: FormData) -> Result<(DatasetForm, Option<DynamicImage>)> {
        let mut form = DatasetForm::default();
        let mut image = None;

        let str = String::from_utf8;

        while let Some(part) = data.next().await {
            let part = part?;
            #[allow(clippy::unnecessary_to_owned)] // We are copying it, basically
            let name: &str = &part.name().to_owned();
            let mut stream = part.stream();

            let mut bytes = Vec::new();
            while let Some(buf) = stream.next().await {
                let mut rdr = buf?.reader();
                rdr.read_to_end(&mut bytes)?;
            }

            match name {
                "id" => form.id = Some(str(bytes)?.parse()?),
                "name" => form.name = str(bytes)?,
                "description" => form.description = str(bytes)?,
                "author" => form.author = str(bytes)?,
                "license" => form.license = str(bytes)?,
                "institution" => form.institution = str(bytes)?,
                "url" => form.url = str(bytes)?,
                "icon" => {
                    // This isn't a guard because we want to avoid the bail! branch
                    if !bytes.is_empty() {
                        image = Some(
                            ImageReader::new(Cursor::new(bytes))
                                .with_guessed_format()?
                                .decode()?,
                        )
                    }
                }
                name => bail!("Invalid field in dataset form {name}"),
            }
        }

        Ok((form, image))
    }
}

impl From<Dataset> for DatasetForm {
    fn from(d: Dataset) -> Self {
        DatasetForm {
            id: Some(d.id),
            name: d.name,
            description: d.description,
            author: d.author,
            license: d.license,
            institution: d.institution.unwrap_or_default(),
            url: d.url.unwrap_or_default(),
        }
    }
}

#[derive(I18nTemplate, Template)]
#[template(path = "site_settings.askama.html")]
struct SiteSettings {
    auth: Auth,
    i18n_info: I18nInfo,
    datasets: Vec<Dataset>,
    previous_success: Result<Action, Action>,
}

#[derive(I18nTemplate, Template)]
#[template(path = "add_dataset.askama.html")]
struct AddDataset {
    auth: Auth,
    i18n_info: I18nInfo,
    dataset: DatasetForm,
}
