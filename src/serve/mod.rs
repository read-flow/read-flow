mod authn;
pub mod models;

use std::{
    io,
    path::{Path, PathBuf},
};

use indexmap::IndexMap;
use rocket::{
    fairing::AdHoc,
    form::Form,
    fs::{NamedFile, TempFile},
    get,
    http::ContentType,
    post, routes,
    serde::{json::Json, Deserialize},
    FromForm, Responder, State,
};

use crate::{
    db::{
        self,
        dao::{self, FileDao, FileTagDao},
        get_connection_pool, ConnectionPool,
    },
    extension_of, scan, to_unique_file,
};

use authn::AuthorizedUser;
use models::File;

#[derive(Debug, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Settings {
    download_folder: PathBuf,
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
    #[error("filename without extension")]
    #[response(status = 400)]
    FilenameWithoutExtension(String),
    #[error("filesystem error: {0}")]
    #[response(status = 500)]
    Io(#[from] io::Error),
    #[error("extension {0} is not supported")]
    #[response(status = 400)]
    UnsupportedExtension(String),
    #[error("could not import file: {0}")]
    #[response(status = 500)]
    Scan(
        String,
        #[response(ignore)]
        #[source]
        scan::Error,
    ),
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

#[rocket::launch]
pub fn serve() -> _ {
    // Get the connection_pool
    let connection_pool = get_connection_pool();

    rocket::build()
        .mount(
            "/",
            routes![
                get_file,
                get_file_tags,
                post_file_tags,
                get_files,
                get_files_tags,
                download_file,
                upload_file,
            ],
        )
        .attach(AdHoc::config::<Settings>())
        .manage(connection_pool)
}

#[get("/files")]
fn get_files(
    connection_pool: &State<ConnectionPool>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<File>>> {
    let files = connection_pool.select_all_files()?;
    let file_tags = connection_pool.select_all_file_tags()?;

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

#[get("/files/tags")]
fn get_files_tags(
    connection_pool: &State<ConnectionPool>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let tags = connection_pool.select_all_tags()?;
    Ok(Json(tags))
}

#[get("/files/<id>")]
fn get_file(
    id: i32,
    connection_pool: &State<ConnectionPool>,
    _user: AuthorizedUser,
) -> Result<Option<Json<File>>> {
    let tags = connection_pool.select_file_tags_by_file_id(id)?;
    let file = connection_pool
        .select_file_by_id(id)?
        .map(|file| (file, tags).into());

    Ok(file.map(Json))
}

#[get("/files/<id>/tags")]
fn get_file_tags(
    id: i32,
    connection_pool: &State<ConnectionPool>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let tags = connection_pool
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
    connection_pool: &State<ConnectionPool>,
    user: AuthorizedUser,
) -> Result<Json<Vec<String>>> {
    let file_tags = tags
        .into_inner()
        .into_iter()
        .map(|tag| db::models::FileTag { file_id: id, tag })
        .collect();
    connection_pool.upsert_file_tags(file_tags)?;

    get_file_tags(id, connection_pool, user)
}

#[get("/files/<id>/download-as/<file_name>")]
async fn download_file(
    id: i32,
    file_name: &str,
    connection_pool: &State<ConnectionPool>,
    _user: AuthorizedUser,
) -> Result<Option<(ContentType, NamedFile)>> {
    let file = connection_pool.select_file_by_id(id)?;

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

            let content_type =
                ContentType::from_extension(&file.type_).unwrap_or_else(|| {
                    match file.type_.to_lowercase().as_str() {
                        "mobi" | "prc" => ContentType::new("application", "x-mobipocket-ebook"),
                        &_ => {
                            tracing::error!("Unsupported file type: {}", file.type_);
                            panic!("Unsupported file type")
                        }
                    }
                });

            Ok(NamedFile::open(path)
                .await
                .ok()
                .map(|file| (content_type, file)))
        }
    }
}

#[derive(Debug, FromForm)]
struct UploadFile<'r> {
    filename: String,
    file: TempFile<'r>,
}

#[post("/files", data = "<form>")]
async fn upload_file(
    mut form: Form<UploadFile<'_>>,
    connection_pool: &State<ConnectionPool>,
    settings: &State<Settings>,
    _user: AuthorizedUser,
) -> Result<Json<File>> {
    // We're only interested in the actual filename, remove any prefixed directories.
    // This also takes care of relative paths, and thus prevents from storing the file outside the download folder.
    let filename = form.filename.split('/').last().unwrap();

    let mut target_file = settings.download_folder.join(filename);

    let extension = extension_of(&form.filename)
        .ok_or(Error::FilenameWithoutExtension(form.filename.to_string()))?
        .to_owned();
    to_unique_file(&mut target_file, &extension);

    if !matches!(extension.to_lowercase().as_str(), "pdf" | "epub" | "mobi") {
        return Err(Error::UnsupportedExtension(extension));
    }

    form.file.persist_to(target_file.clone()).await?;

    let visitor = scan::create_visitor(connection_pool.inner().clone());
    visitor.visit(&target_file)?;

    let result = connection_pool
        .select_file_by_path(&format!("{}", target_file.display()))?
        .unwrap();
    Ok(Json((result, vec![]).into()))
}
