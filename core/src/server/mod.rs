mod authn;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use authn::AuthorizedUser;
use authn::PrivateModeHeader;
use figment::Figment;
use provider::r#async::AndThen;
use provider::r#async::Provider;
use rocket::Build;
use rocket::Ignite;
use rocket::Responder;
use rocket::Rocket;
use rocket::State;
use rocket::delete;
use rocket::form::Form;
use rocket::fs::NamedFile;
use rocket::fs::TempFile;
use rocket::get;
use rocket::http::ContentType;
use rocket::http::Method;
use rocket::post;
use rocket::put;
use rocket::routes;
use rocket::serde::json::Json;
use rocket_cors::AllowedOrigins;
use rocket_cors::Cors;
use rocket_cors::CorsOptions;

use crate::ApplicationModule;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingProgress;
use crate::api::Status;
use crate::db::dao;
use crate::settings;
pub use crate::settings::ServerSettings;
use crate::settings::Settings;
use crate::settings::SettingsError;
use crate::to_unique_file;

#[derive(Debug, thiserror::Error, Responder)]
enum Error {
    #[error("database error: {0}")]
    #[response(status = 500)]
    Dao(
        String,
        #[response(ignore)]
        #[source]
        dao::Error,
    ),
    #[error("filesystem error: {0}")]
    #[response(status = 500)]
    Io(#[from] io::Error),
    #[error("extension {0} is not supported")]
    #[response(status = 400)]
    UnsupportedExtension(String),
    #[error("content-type {0} is not supported")]
    #[response(status = 400)]
    UnsupportedContentType(String),
    #[error("could not import file: {0}")]
    #[response(status = 500)]
    Scan(String),
    #[error("file with guid {0} not found")]
    #[response(status = 404)]
    FileNotFound(String),
    #[error("private mode access requires owner role")]
    #[response(status = 403)]
    Forbidden(String),
}

impl From<dao::Error> for Error {
    fn from(error: dao::Error) -> Self {
        tracing::error!("database error: {error}");
        Error::Dao(error.to_string(), error)
    }
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!("could not import file: {error}");
        Error::Scan(error.to_string())
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

pub fn create_cors() -> Cors {
    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_methods(
            vec![
                Method::Get,
                Method::Post,
                Method::Put,
                Method::Options,
                Method::Delete,
            ]
            .into_iter()
            .map(From::from)
            .collect(),
        )
        .allowed_headers(rocket_cors::AllowedHeaders::All)
        .allow_credentials(true);

    cors.to_cors().unwrap()
}

struct FigmentProvider {
    config_path: PathBuf,
}

impl Provider<Figment> for FigmentProvider {
    type Error = SettingsError;
    async fn provide(&self) -> Result<Figment, Self::Error> {
        Ok(settings::decorate_with(
            rocket::Config::figment(),
            self.config_path.clone(),
        ))
    }
}

type SettingsProvider =
    AndThen<FigmentProvider, fn(Figment) -> Result<Settings, SettingsError>, Figment>;

fn extract_settings(figment: Figment) -> Result<Settings, SettingsError> {
    Ok(figment.extract()?)
}

async fn serve(config_path: PathBuf) -> Rocket<Build> {
    let figment_provider = FigmentProvider {
        config_path: config_path.clone(),
    };
    // unwrap is safe because FigmentProvider technically doesn't err
    let figment = figment_provider.provide().await.unwrap();

    let settings_provider = figment_provider
        .and_then(extract_settings as fn(Figment) -> Result<Settings, SettingsError>);

    let application_module: ApplicationModule<SettingsProvider> =
        ApplicationModule::new(settings_provider, config_path)
            .await
            .expect("extract settings");

    let cors = create_cors();

    let routes = routes![
        status,
        get_file,
        update_file,
        get_file_tags,
        post_file_tags,
        delete_file_tags,
        get_files,
        get_files_tags,
        download_file,
        upload_file,
        delete_file,
        get_reading_progress,
        put_reading_progress,
    ];

    rocket::custom(figment)
        .mount("/", routes)
        .manage(application_module)
        .attach(cors)
}

pub async fn main(config_path: PathBuf) -> Result<Rocket<Ignite>, Box<rocket::Error>> {
    Ok(serve(config_path).await.launch().await?)
}

#[get("/status")]
async fn status(
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
) -> Result<Json<Status>> {
    let db_status = application_module.db_client().await.status().await?;
    let status = Status {
        identifier: "server".to_string(),
        attributes: HashMap::from_iter([("user_id".to_string(), user.user_id)]),
        nested_checks: vec![db_status],
    };
    Ok(Json(status))
}

#[get("/files")]
async fn get_files(
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<File>>> {
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
        let files = application_module.db_client().await.get_files().await?;
        return Ok(Json(files));
    }

    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let excluded = settings.ui.private_tags().to_vec();
    let db_files = dao::select_all_files_excluding_tags(&mut conn, &excluded).await?;
    let all_tags = dao::select_all_content_tags(&mut conn).await?;
    let mut tags_by_fp: std::collections::HashMap<String, Vec<crate::db::models::ContentTag>> =
        std::collections::HashMap::new();
    for tag in all_tags {
        tags_by_fp
            .entry(tag.fingerprint.clone())
            .or_default()
            .push(tag);
    }
    let files = db_files
        .into_iter()
        .map(|file| {
            let tags = tags_by_fp.remove(&file.fingerprint).unwrap_or_default();
            (file, tags).into()
        })
        .collect();
    Ok(Json(files))
}

#[put("/files", data = "<file>")]
async fn update_file(
    file: Json<File>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Json<File>> {
    application_module
        .db_client()
        .await
        .update_file(file.0.clone())
        .await?;
    Ok(file)
}

#[get("/files/tags")]
async fn get_files_tags(
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<String>>> {
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
        let pool = application_module.connection_pool().await;
        let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
        let tags = dao::select_all_distinct_tags(&mut conn).await?;
        return Ok(Json(tags));
    }
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let excluded = settings.ui.private_tags().to_vec();
    let tags = dao::select_all_distinct_tags_excluding(&mut conn, &excluded).await?;
    Ok(Json(tags))
}

#[get("/files/<guid>")]
async fn get_file(
    guid: &str,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Option<Json<File>>> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(None);
    };
    let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings
        .ui
        .contains_hidden_tag(&tags.iter().map(|t| t.tag.clone()).collect::<Vec<_>>())
    {
        return Ok(None);
    }
    Ok(Some(Json((file, tags).into())))
}

#[get("/files/<guid>/tags")]
async fn get_file_tags(
    guid: &str,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<String>>> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(Json(vec![]));
    };
    let content_tags =
        dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
    let tag_strings: Vec<String> = content_tags.iter().map(|t| t.tag.clone()).collect();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&tag_strings) {
        return Ok(Json(vec![]));
    }
    Ok(Json(tag_strings))
}

#[post("/files/<guid>/tags", data = "<tags>")]
async fn post_file_tags(
    guid: &str,
    tags: Json<Vec<String>>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<String>>> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(Json(vec![]));
    };
    let existing_tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
        .await?
        .iter()
        .map(|t| t.tag.clone())
        .collect::<Vec<_>>();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&existing_tags) {
        return Ok(Json(vec![]));
    }
    let content_tags = tags
        .into_inner()
        .into_iter()
        .map(|tag| crate::db::models::ContentTag::new(file.fingerprint.clone(), tag))
        .collect();
    dao::upsert_many_content_tags(&mut conn, content_tags).await?;
    get_file_tags(guid, application_module, user, private_mode).await
}

#[delete("/files/<guid>/tags", data = "<tags>")]
async fn delete_file_tags(
    guid: &str,
    tags: Json<Vec<String>>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<String>>> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(Json(vec![]));
    };
    let existing_tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
        .await?
        .iter()
        .map(|t| t.tag.clone())
        .collect::<Vec<_>>();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&existing_tags) {
        return Ok(Json(vec![]));
    }
    dao::delete_content_tags(&mut conn, &file.fingerprint, tags.into_inner()).await?;
    get_file_tags(guid, application_module, user, private_mode).await
}

#[get("/files/<guid>/download-as/<file_name>")]
async fn download_file(
    guid: &str,
    file_name: &str,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Option<(ContentType, NamedFile)>> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let file = dao::select_file_by_guid(&mut conn, guid).await?;

    match file {
        None => Ok(None),
        Some(file) => {
            let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
                .await?
                .iter()
                .map(|t| t.tag.clone())
                .collect::<Vec<_>>();
            if private_mode.0 {
                if !user.has_role("owner") {
                    return Err(Error::Forbidden(
                        "private mode access requires owner role".into(),
                    ));
                }
            } else if settings.ui.contains_hidden_tag(&tags) {
                return Ok(None);
            }
            if !file_name.ends_with(&file.type_.to_lowercase()) {
                tracing::error!(
                    "Incorrect file extension on `{file_name}`, expected `{}`",
                    file.type_
                );
                return Ok(None);
            }

            let path = Path::new(&file.path);
            if !path.exists() {
                tracing::error!("Database out of sync, file not found: {path:?}");
                return Ok(None);
            }

            let content_type = extension_to_content_type(&file.type_)?;

            Ok(NamedFile::open(path)
                .await
                .ok()
                .map(|file| (content_type, file)))
        }
    }
}

#[delete("/files/<guid>")]
async fn delete_file(
    guid: &str,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<()> {
    let settings = application_module.settings().await;
    let db_client = application_module.db_client().await;
    let file = db_client.get_file(guid).await?;

    if let Some(ref file) = file {
        if private_mode.0 {
            if !user.has_role("owner") {
                return Err(Error::Forbidden(
                    "private mode access requires owner role".into(),
                ));
            }
        } else if settings.ui.contains_hidden_tag(&file.tags) {
            return Err(Error::FileNotFound(guid.to_string()));
        }
        db_client.delete_file(file.clone()).await?;
        Ok(())
    } else {
        Err(Error::FileNotFound(guid.to_string()))
    }
}

#[post("/files", data = "<file>")]
async fn upload_file(
    mut file: Form<TempFile<'_>>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Json<File>> {
    let extension = file
        .content_type()
        .map(content_type_to_extension)
        .transpose()?
        .unwrap();

    if !matches!(extension.to_lowercase().as_str(), "pdf" | "epub" | "mobi") {
        return Err(Error::UnsupportedExtension(extension));
    }

    let filename = file.name().unwrap(); // sanitized filename, safe to use
    let target_dir = application_module
        .settings()
        .await
        .server
        .download_folder
        .join(filename);

    if !target_dir.exists() {
        tokio::fs::create_dir(&target_dir).await?;
    }

    let mut target_file = target_dir.join(format!("{filename}.{extension}"));

    to_unique_file(&mut target_file, &extension);

    file.persist_to(target_file.as_path()).await?;

    application_module.scan(target_file.clone()).await?;

    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let result = dao::select_file_by_path(&mut conn, &target_file.display().to_string())
        .await?
        .unwrap();
    Ok(Json((result, vec![]).into()))
}

#[get("/reading-progress/<fingerprint>")]
async fn get_reading_progress(
    fingerprint: &str,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Option<Json<ReadingProgress>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let progress = dao::get_reading_progress(&mut conn, fingerprint).await?;
    Ok(progress.map(Json))
}

#[put("/reading-progress", data = "<progress>")]
async fn put_reading_progress(
    progress: Json<ReadingProgress>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<()> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    dao::upsert_reading_progress(&mut conn, progress.into_inner()).await?;
    Ok(())
}

fn extension_to_content_type(extension: &str) -> Result<ContentType> {
    ContentType::from_extension(extension)
        .or_else(|| match extension.to_lowercase().as_str() {
            "mobi" | "prc" => ContentType::new("application", "x-mobipocket-ebook").into(),
            &_ => None,
        })
        .ok_or(Error::UnsupportedExtension(extension.to_string()))
}

fn content_type_to_extension(content_type: &ContentType) -> Result<String> {
    content_type
        .extension()
        .map(|ext| ext.as_str().to_owned())
        .or_else(|| {
            (content_type.top() == "application" && content_type.sub() == "x-mobipocket-ebook")
                .then(|| "mobi".to_owned())
        })
        .ok_or(Error::UnsupportedContentType(content_type.to_string()))
}
