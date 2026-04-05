mod authn;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use authn::AuthorizedUser;
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
use crate::FxIndexMap;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingProgress;
use crate::api::Status;
use crate::db;
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
    #[error("file with id {0} not found")]
    #[response(status = 404)]
    FileNotFound(String),
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

pub fn main(config_path: PathBuf) -> Result<Rocket<Ignite>, Box<rocket::Error>> {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let result = rt.block_on(async { serve(config_path).await.launch().await })?;
    Ok(result)
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
    _user: AuthorizedUser,
) -> Result<Json<Vec<File>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let files = dao::select_all_files(&mut *conn).await?;
    let file_tags = dao::select_all_file_tags(&mut *conn).await?;

    let mut file_tags_map: FxIndexMap<_, Vec<_>> = FxIndexMap::default();

    for file_tag in file_tags {
        match file_tags_map.get_mut(&file_tag.file_id) {
            Some(tags) => {
                tags.push(file_tag);
            }
            None => {
                file_tags_map.insert(file_tag.file_id, vec![file_tag]);
            }
        };
    }

    let models: Vec<File> = files
        .into_iter()
        .map(|f| {
            let tags = file_tags_map.get(&f.id).cloned().unwrap_or(vec![]);
            (f, tags).into()
        })
        .collect();

    Ok(Json(models))
}

#[put("/files", data = "<file>")]
async fn update_file(
    file: Json<File>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Json<File>> {
    let (db_file, _) = file.0.clone().into();
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    dao::update_file(&mut *conn, db_file).await?;

    Ok(file)
}

#[get("/files/tags")]
async fn get_files_tags(
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let tags = dao::select_all_tags(&mut *conn).await?;
    Ok(Json(tags))
}

#[get("/files/<id>")]
async fn get_file(
    id: i32,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Option<Json<File>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let tags = dao::select_file_tags_by_file_id(&mut *conn, id).await?;
    let file = dao::select_file_by_id(&mut *conn, id)
        .await?
        .map(|file| (file, tags).into());

    Ok(file.map(Json))
}

#[get("/files/<id>/tags")]
async fn get_file_tags(
    id: i32,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let tags = dao::select_file_tags_by_file_id(&mut *conn, id)
        .await?
        .into_iter()
        .map(|tag| tag.tag)
        .collect();

    Ok(Json(tags))
}

#[post("/files/<id>/tags", data = "<tags>")]
async fn post_file_tags(
    id: i32,
    tags: Json<Vec<String>>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let file_tags = tags
        .into_inner()
        .into_iter()
        .map(|tag| db::models::FileTag::new(id, tag))
        .collect();
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    dao::upsert_many_file_tags(&mut conn, file_tags).await?;

    get_file_tags(id, application_module, user).await
}

#[delete("/files/<id>/tags", data = "<tags>")]
async fn delete_file_tags(
    id: i32,
    tags: Json<Vec<String>>,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    dao::delete_file_tags(&mut conn, id, tags.into_inner()).await?;
    get_file_tags(id, application_module, user).await
}

#[get("/files/<id>/download-as/<file_name>")]
async fn download_file(
    id: i32,
    file_name: &str,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<Option<(ContentType, NamedFile)>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let file = dao::select_file_by_id(&mut *conn, id).await?;

    match file {
        None => Ok(None),
        Some(file) => {
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

#[delete("/files/<id>")]
async fn delete_file(
    id: i32,
    application_module: &State<ApplicationModule<SettingsProvider>>,
    _user: AuthorizedUser,
) -> Result<()> {
    let db_client = application_module.db_client().await;
    // Get the file to delete
    let file = db_client.get_file(id).await?;

    if let Some(file) = file {
        // Delete the file from the database
        db_client.delete_file(file).await?;

        Ok(())
    } else {
        Err(Error::FileNotFound(id.to_string()))
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
    let result = dao::select_file_by_path(&mut *conn, &target_file.display().to_string())
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
    let progress = dao::get_reading_progress(&mut *conn, fingerprint).await?;
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
    dao::upsert_reading_progress(&mut *conn, progress.into_inner()).await?;
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
