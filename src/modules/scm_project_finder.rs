use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, Pool},
};
use rayon::prelude::*;

use crate::{models::NewDirectory, schema::directories};

use super::{DirectoryError, DirectoryModule};

pub struct ScmProjectFinder {
    /// The hidden SCM directory, e.g. `.git`, `.hg`
    directory: String,
    projects: Mutex<Vec<PathBuf>>,
    connection_pool: Pool<ConnectionManager<SqliteConnection>>,
}

impl ScmProjectFinder {
    pub fn new(
        directory: String,
        connection_pool: Pool<ConnectionManager<SqliteConnection>>,
    ) -> Self {
        Self {
            directory,
            projects: vec![].into(),
            connection_pool,
        }
    }
}

impl DirectoryModule for ScmProjectFinder {
    fn matches(&self, directory: &Path) -> bool {
        directory.join(&self.directory).is_dir()
    }

    fn handle(&self, directory: &Path) -> Result<(), DirectoryError> {
        let mut projects = self.projects.lock().unwrap();
        projects.push(directory.to_owned());
        Ok(())
    }

    fn finalize(&self) -> Result<(), DirectoryError> {
        let projects = self.projects.lock().unwrap();
        // the following assumes that the directory is hidden, and removes the '.'
        let directory = &self.directory.to_ascii_uppercase()[1..].to_string();
        println!("{directory} projects found: {projects:?}");

        let entities: Vec<_> = projects
            .par_iter()
            .map(|dir| NewDirectory {
                path: format!("{}", dir.display()),
                type_: directory.clone(),
            })
            .collect();

        let mut connection = self.connection_pool.get()?;

        diesel::insert_into(directories::table)
            .values(entities)
            .execute(&mut connection)?;

        println!("directories added to the database");

        Ok(())
    }
}
