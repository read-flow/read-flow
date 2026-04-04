use itertools::Itertools;
use itertools::concat;
use provider::r#async::Provider;

use crate::ApplicationModule;
use crate::db::dao;
use crate::db::dao::Error;
use crate::db::models::File;
use crate::db::models::FileTag;
use crate::scan::ScanSettings;
use crate::settings::Settings;
use crate::settings::SettingsError;

impl<P> ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    pub async fn apply_tags(&self) -> Result<(), Error> {
        let settings = self.settings().await;
        self.apply_tags_from_settings(&settings.scan).await
    }

    async fn apply_tags_from_settings(&self, scan_settings: &ScanSettings) -> Result<(), Error> {
        let pool = self.connection_pool().await;
        let mut file_tags_to_add: Vec<Vec<FileTag>> = Vec::new();

        for (path, tags) in &scan_settings.auto_tags {
            let files = dao::select_all_files_by_path_like(&pool, path).await?;
            if scan_settings.dry_run {
                for file in files.iter() {
                    println!("{}: {:?}", file.path, tags);
                }
            }
            file_tags_to_add.push(to_all_file_tags(files, tags));
        }

        if !scan_settings.dry_run {
            let mut conn = pool.acquire().await?;
            dao::upsert_many_file_tags(&mut conn, concat(file_tags_to_add)).await?;
        }
        Ok(())
    }
}

fn to_all_file_tags(files: Vec<File>, tags: &Vec<String>) -> Vec<FileTag> {
    files
        .into_iter()
        .cartesian_product(tags)
        .map(|(file, tag)| FileTag::new(file.id, tag.clone()))
        .collect::<Vec<FileTag>>()
}
