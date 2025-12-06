mod authn;

use std::io;
use std::path::Path;

use authn::AuthorizedUser;
use indexmap::IndexMap;
use rocket::Responder;
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
use rocket::serde::Deserialize;
use rocket::serde::Serialize;
use rocket::serde::json::Json;
use rocket_cors::AllowedOrigins;
use rocket_cors::Cors;
use rocket_cors::CorsOptions;

use crate::ApplicationModule;
use crate::ExpandedPath;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::Status;
use crate::db;
use crate::db::dao;
use crate::db::dao::FileDao;
use crate::db::dao::FileTagDao;
use crate::db::datasource::DbClient;
use crate::scan;
use crate::settings;
use crate::to_unique_file;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(crate = "rocket::serde")]
pub struct ServerSettings {
    pub download_folder: ExpandedPath,
    pub authorization_tokens: Vec<String>,
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
}

fn default_jwt_secret() -> String {
    "archive_organizer_jwt_secret".to_string()
}

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
    Scan(
        String,
        #[response(ignore)]
        #[source]
        scan::Error,
    ),
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

impl From<scan::Error> for Error {
    fn from(error: scan::Error) -> Self {
        tracing::error!("could not import file: {error}");
        Error::Scan(error.to_string(), error)
    }
}

type Result<T> = std::result::Result<T, Error>;

pub fn create_cors() -> Cors {
    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_methods(
            vec![Method::Get, Method::Post, Method::Options, Method::Delete]
                .into_iter()
                .map(From::from)
                .collect(),
        )
        .allowed_headers(rocket_cors::AllowedHeaders::All)
        .allow_credentials(true);

    cors.to_cors().unwrap()
}

#[rocket::launch]
pub fn serve() -> _ {
    let figment = settings::decorate(rocket::Config::figment());

    let application_module = ApplicationModule::from_figment(&figment).unwrap();

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
    ];

    rocket::custom(figment)
        .mount("/", routes)
        .manage(application_module)
        .attach(cors)
}

#[get("/status")]
async fn status(
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Json<Status>> {
    let status = application_module.db_client().status().await?;
    Ok(Json(status))
}

#[get("/files")]
fn get_files(
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<File>>> {
    let files = application_module.connection_pool.select_all_files()?;
    let file_tags = application_module.connection_pool.select_all_file_tags()?;

    let mut file_tags_map: IndexMap<_, Vec<_>> = IndexMap::new();

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
fn update_file(
    file: Json<File>,
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Json<File>> {
    let (db_file, _) = file.0.clone().into();
    application_module.connection_pool.update_file(db_file)?;

    Ok(file)
}

#[get("/files/tags")]
fn get_files_tags(
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let tags = application_module.connection_pool.select_all_tags()?;
    Ok(Json(tags))
}

#[get("/files/<id>")]
fn get_file(
    id: i32,
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Option<Json<File>>> {
    let tags = application_module
        .connection_pool
        .select_file_tags_by_file_id(id)?;
    let file = application_module
        .connection_pool
        .select_file_by_id(id)?
        .map(|file| (file, tags).into());

    Ok(file.map(Json))
}

#[get("/files/<id>/tags")]
fn get_file_tags(
    id: i32,
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let tags = application_module
        .connection_pool
        .select_file_tags_by_file_id(id)?
        .into_iter()
        .map(|tag| tag.tag)
        .collect();
    Ok(Json(tags))
}

#[post("/files/<id>/tags", data = "<tags>")]
fn post_file_tags(
    id: i32,
    tags: Json<Vec<String>>,
    application_module: &State<ApplicationModule>,
    user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let file_tags = tags
        .into_inner()
        .into_iter()
        .map(|tag| db::models::FileTag::new(id, tag))
        .collect();
    application_module
        .connection_pool
        .upsert_many_file_tags(file_tags)?;

    get_file_tags(id, application_module, user)
}

#[delete("/files/<id>/tags", data = "<tags>")]
fn delete_file_tags(
    id: i32,
    tags: Json<Vec<String>>,
    application_module: &State<ApplicationModule>,
    user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    for tag in tags.into_inner() {
        application_module
            .connection_pool
            .delete_file_tag(db::models::FileTag::new(id, tag))?;
    }

    get_file_tags(id, application_module, user)
}

#[get("/files/<id>/download-as/<file_name>")]
async fn download_file(
    id: i32,
    file_name: &str,
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<Option<(ContentType, NamedFile)>> {
    let file = application_module.connection_pool.select_file_by_id(id)?;

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
    application_module: &State<ApplicationModule>,
    _user: AuthorizedUser,
) -> Result<()> {
    let db_client = DbClient::new(application_module.connection_pool.clone());
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
    application_module: &State<ApplicationModule>,
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
        .settings
        .server
        .download_folder
        .join(filename);

    if !target_dir.exists() {
        tokio::fs::create_dir(&target_dir).await?;
    }

    let mut target_file = target_dir.join(format!("{filename}.{extension}"));

    to_unique_file(&mut target_file, &extension);

    file.persist_to(target_file.as_path()).await?;

    application_module.visitor().visit(&target_file)?;

    let result = application_module
        .connection_pool
        .select_file_by_path(&target_file.display().to_string())?
        .unwrap();
    Ok(Json((result, vec![]).into()))
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
