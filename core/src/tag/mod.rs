// SPDX-License-Identifier: AGPL-3.0-or-later

use itertools::Itertools;
use itertools::concat;
use provider::r#async::Provider;

use crate::ApplicationModule;
use crate::db::dao;
use crate::db::dao::Error;
use crate::db::models::ContentTag;
use crate::db::models::File;
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
        let mut conn = pool.acquire().await?;
        let mut tags_to_add: Vec<Vec<ContentTag>> = Vec::new();

        for (path, tags) in &scan_settings.auto_tags {
            let files = dao::select_all_files_by_path_like(&mut conn, path).await?;
            if scan_settings.dry_run {
                for file in files.iter() {
                    println!("{}: {:?}", file.path, tags);
                }
            }
            tags_to_add.push(to_all_content_tags(files, tags));
        }

        if !scan_settings.dry_run {
            dao::upsert_many_content_tags(&mut conn, concat(tags_to_add)).await?;
        }
        Ok(())
    }
}

fn to_all_content_tags(files: Vec<File>, tags: &Vec<String>) -> Vec<ContentTag> {
    files
        .into_iter()
        .cartesian_product(tags)
        .map(|(file, tag)| ContentTag::new(file.fingerprint, tag.clone()))
        .collect()
}
