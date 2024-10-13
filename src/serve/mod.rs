use std::path::Path;

use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SelectableHelper};
use rocket::{fs::NamedFile, get, http::ContentType, routes, serde::json::Json, State};

use crate::db::{
    get_connection_pool,
    models::{File, FileTag},
    schema::{file_tags, files},
    ConnectionPool,
};

#[rocket::launch]
pub fn serve() -> _ {
    // Get the connection_pool
    let connection_pool = get_connection_pool();

    rocket::build()
        .mount(
            "/",
            routes![
                list_files,
                list_files_tags,
                list_file,
                list_file_tags,
                download_file
            ],
        )
        .manage(connection_pool)
}

#[get("/files")]
fn list_files(connection_pool: &State<ConnectionPool>) -> Json<Vec<File>> {
    let mut connection = connection_pool.get().unwrap();
    let files = files::table.load(&mut connection).unwrap();
    Json(files)
}

#[get("/files/tags")]
fn list_files_tags(connection_pool: &State<ConnectionPool>) -> Json<Vec<FileTag>> {
    let mut connection = connection_pool.get().unwrap();
    let file_tags = file_tags::table.load(&mut connection).unwrap();
    Json(file_tags)
}

#[get("/files/<id>")]
fn list_file(connection_pool: &State<ConnectionPool>, id: i32) -> Option<Json<File>> {
    let mut connection = connection_pool.get().unwrap();
    let file = files::table
        .find(id)
        .select(File::as_select())
        .first(&mut connection)
        .optional()
        .unwrap();
    file.map(Json)
}

#[get("/files/<id>/tags")]
fn list_file_tags(connection_pool: &State<ConnectionPool>, id: i32) -> Json<Vec<FileTag>> {
    let mut connection = connection_pool.get().unwrap();
    let file_tags = file_tags::table
        .filter(file_tags::file_id.eq(id))
        .select(FileTag::as_select())
        .load(&mut connection)
        .unwrap();
    Json(file_tags)
}

#[get("/files/<id>/download-as/<file_name>")]
async fn download_file(
    connection_pool: &State<ConnectionPool>,
    id: i32,
    file_name: &str,
) -> Option<(ContentType, NamedFile)> {
    let mut connection = connection_pool.get().unwrap();
    let file: Option<File> = files::table
        .find(id)
        .select(File::as_select())
        .first(&mut connection)
        .optional()
        .unwrap();

    match file {
        None => None,
        Some(file) => {
            if !file_name.ends_with(&file.type_.to_lowercase()) {
                tracing::error!(
                    "Incorrect file extension on `{file_name}`, expected `{}`",
                    file.type_
                );
                return None;
            }
            let path = Path::new(&file.path);
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
            NamedFile::open(path)
                .await
                .ok()
                .map(|file| (content_type, file))
        }
    }
}
