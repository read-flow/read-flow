use std::sync::Arc;

use diesel::prelude::*;

use crate::db::{
    models::{File, FileTag},
    schema::{file_tags, files},
    ConnectionPool,
};

pub trait FileDao {
    type Error;
    fn select_all_files(&self) -> Result<Vec<File>, Self::Error>;
    fn select_all_files_order_by_id(&self) -> Result<Vec<File>, Self::Error>;
    fn select_all_files_order_by_type(&self) -> Result<Vec<File>, Self::Error>;
    fn select_all_files_order_by_path(&self) -> Result<Vec<File>, Self::Error>;
    fn select_all_files_order_by_size(&self) -> Result<Vec<File>, Self::Error>;
    fn select_all_files_order_by_sha256sum(&self) -> Result<Vec<File>, Self::Error>;
    fn select_file_by_id(&self, id: i32) -> Result<Option<File>, Self::Error>;
}

pub trait FileTagDao {
    type Error;
    fn insert_file_tag(&self, file_tag: FileTag) -> Result<FileTag, Self::Error>;
    fn select_all_file_tags(&self) -> Result<Vec<FileTag>, Self::Error>;
    fn select_file_tags_by_file_id(&self, file_id: i32) -> Result<Vec<FileTag>, Self::Error>;
    fn select_file_tags_by_tag(&self, tag: &str) -> Result<Vec<FileTag>, Self::Error>;
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Diesel(#[source] Arc<diesel::result::Error>),
    #[error("connection pool error: {0}")]
    R2D2(#[source] Arc<r2d2::Error>),
}

impl From<diesel::result::Error> for Error {
    fn from(value: diesel::result::Error) -> Self {
        Self::Diesel(Arc::new(value))
    }
}

impl From<r2d2::Error> for Error {
    fn from(value: r2d2::Error) -> Self {
        Self::R2D2(Arc::new(value))
    }
}

impl FileDao for ConnectionPool {
    type Error = Error;

    fn select_all_files(&self) -> Result<Vec<File>, Self::Error> {
        let mut connection = self.get()?;
        let files = files::table.load(&mut connection)?;
        Ok(files)
    }

    fn select_all_files_order_by_id(&self) -> Result<Vec<File>, Self::Error> {
        let mut connection = self.get()?;
        let files = files::table
            .order_by(files::columns::id)
            .load(&mut connection)?;
        Ok(files)
    }

    fn select_all_files_order_by_type(&self) -> Result<Vec<File>, Self::Error> {
        let mut connection = self.get()?;
        let files = files::table
            .order_by(files::columns::type_)
            .load(&mut connection)?;
        Ok(files)
    }

    fn select_all_files_order_by_path(&self) -> Result<Vec<File>, Self::Error> {
        let mut connection = self.get()?;
        let files = files::table
            .order_by(files::columns::path)
            .load(&mut connection)?;
        Ok(files)
    }

    fn select_all_files_order_by_size(&self) -> Result<Vec<File>, Self::Error> {
        let mut connection = self.get()?;
        let files = files::table
            .order_by(files::columns::size)
            .load(&mut connection)?;
        Ok(files)
    }

    fn select_all_files_order_by_sha256sum(&self) -> Result<Vec<File>, Self::Error> {
        let mut connection = self.get()?;
        let files = files::table
            .order_by(files::columns::sha256sum)
            .load(&mut connection)?;
        Ok(files)
    }

    fn select_file_by_id(&self, id: i32) -> Result<Option<File>, Self::Error> {
        let mut connection = self.get()?;
        let file = files::table
            .find(id)
            .select(File::as_select())
            .first(&mut connection)
            .optional()?;
        Ok(file)
    }
}

impl FileTagDao for ConnectionPool {
    type Error = Error;

    fn insert_file_tag(&self, file_tag: FileTag) -> Result<FileTag, Self::Error> {
        let mut connection = self.get()?;
        let file_tag = diesel::insert_into(file_tags::table)
            .values(&file_tag)
            .returning(FileTag::as_returning())
            .get_result(&mut connection)?;
        Ok(file_tag)
    }

    fn select_all_file_tags(&self) -> Result<Vec<FileTag>, Self::Error> {
        let mut connection = self.get()?;
        let file_tags = file_tags::table.load(&mut connection)?;
        Ok(file_tags)
    }

    fn select_file_tags_by_file_id(&self, file_id: i32) -> Result<Vec<FileTag>, Self::Error> {
        let mut connection = self.get()?;
        let file_tags = file_tags::table
            .filter(file_tags::file_id.eq(file_id))
            .select(FileTag::as_select())
            .load(&mut connection)?;
        Ok(file_tags)
    }

    fn select_file_tags_by_tag(&self, tag: &str) -> Result<Vec<FileTag>, Self::Error> {
        let mut connection = self.get()?;
        let file_tags = file_tags::table
            .filter(file_tags::tag.eq(tag))
            .select(FileTag::as_select())
            .load(&mut connection)?;
        Ok(file_tags)
    }
}
