mod authn;
mod models;

use std::path::Path;

use indexmap::IndexMap;
use rocket::{fs::NamedFile, get, http::ContentType, post, routes, serde::json::Json, State};

use crate::db::{
    self,
    dao::{FileDao, FileTagDao},
    get_connection_pool, ConnectionPool,
};

use authn::AuthorizedUser;
use models::File;

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
                download_file
            ],
        )
        .manage(connection_pool)
}

#[get("/files")]
fn get_files(connection_pool: &State<ConnectionPool>, _user: AuthorizedUser) -> Json<Vec<File>> {
    let files = connection_pool.select_all_files().unwrap();
    let file_tags = connection_pool.select_all_file_tags().unwrap();

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

    Json(models)
}

#[get("/files/tags")]
fn get_files_tags(
    connection_pool: &State<ConnectionPool>,
    _user: AuthorizedUser,
) -> Json<Vec<String>> {
    let tags = connection_pool.select_all_tags().unwrap();
    Json(tags)
}

#[get("/files/<id>")]
fn get_file(
    connection_pool: &State<ConnectionPool>,
    id: i32,
    _user: AuthorizedUser,
) -> Option<Json<File>> {
    let file: Option<File> = connection_pool.select_file_by_id(id).unwrap().map(|file| {
        let tags = connection_pool
            .select_file_tags_by_file_id(file.id)
            .unwrap();
        (file, tags).into()
    });

    file.map(Json)
}

#[get("/files/<id>/tags")]
fn get_file_tags(
    connection_pool: &State<ConnectionPool>,
    id: i32,
    _user: AuthorizedUser,
) -> Json<Vec<String>> {
    let tags = connection_pool
        .select_file_tags_by_file_id(id)
        .unwrap()
        .into_iter()
        .map(|tag| tag.tag)
        .collect();
    Json(tags)
}

#[post("/files/<id>/tags", data = "<tags>")]
fn post_file_tags(
    connection_pool: &State<ConnectionPool>,
    id: i32,
    tags: Json<Vec<String>>,
    user: AuthorizedUser,
) -> Json<Vec<String>> {
    let file_tags = tags
        .into_inner()
        .into_iter()
        .map(|tag| db::models::FileTag { file_id: id, tag })
        .collect();
    connection_pool.upsert_file_tags(file_tags).unwrap();

    get_file_tags(connection_pool, id, user)
}

#[get("/files/<id>/download-as/<file_name>")]
async fn download_file(
    connection_pool: &State<ConnectionPool>,
    id: i32,
    file_name: &str,
    _user: AuthorizedUser,
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
