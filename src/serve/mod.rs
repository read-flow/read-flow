use std::path::Path;

use rocket::{fs::NamedFile, get, http::ContentType, routes, serde::json::Json, State};

use crate::db::{
    dao::{FileDao, FileTagDao},
    get_connection_pool,
    models::{File, FileTag},
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
    let files = connection_pool.select_all_files().unwrap();
    Json(files)
}

#[get("/files/tags")]
fn list_files_tags(connection_pool: &State<ConnectionPool>) -> Json<Vec<FileTag>> {
    let file_tags = connection_pool.select_all_file_tags().unwrap();
    Json(file_tags)
}

#[get("/files/<id>")]
fn list_file(connection_pool: &State<ConnectionPool>, id: i32) -> Option<Json<File>> {
    let file = connection_pool.select_file_by_id(id).unwrap();
    file.map(Json)
}

#[get("/files/<id>/tags")]
fn list_file_tags(connection_pool: &State<ConnectionPool>, id: i32) -> Json<Vec<FileTag>> {
    let file_tags = connection_pool.select_file_tags_by_file_id(id).unwrap();
    Json(file_tags)
}

#[get("/files/<id>/download-as/<file_name>")]
async fn download_file(
    connection_pool: &State<ConnectionPool>,
    id: i32,
    file_name: &str,
) -> Option<(ContentType, NamedFile)> {
    let file = connection_pool.select_file_by_id(id).unwrap();

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
