use itertools::Itertools;
use itertools::concat;
use provider::sync::Provider;

use crate::ApplicationModule;
use crate::db::ConnectionPoolExt;
use crate::db::dao;
use crate::db::dao::Error;
use crate::db::models::File;
use crate::db::models::FileTag;
use crate::scan::ScanSettings;
use crate::settings::Settings;
use crate::settings::SettingsError;

impl<P> ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError>,
{
    pub fn apply_tags(&self) -> Result<(), Error> {
        self.apply_tags_from_settings(&self.settings().scan)
    }

    fn apply_tags_from_settings(&self, scan_settings: &ScanSettings) -> Result<(), Error> {
        let file_tags_to_add = scan_settings
            .auto_tags
            .iter()
            .map(|(path, tags)| {
                self.connection_pool()
                    .with_connection(|conn| dao::select_all_files_by_path_like(conn, path))
                    .map(|files| {
                        if scan_settings.dry_run {
                            for file in files.iter() {
                                println!("{}: {:?}", file.path, tags);
                            }
                        }
                        to_all_file_tags(files, tags)
                    })
            })
            .collect::<Result<Vec<Vec<FileTag>>, Error>>()?;

        if scan_settings.dry_run {
            Ok(())
        } else {
            self.connection_pool()
                .with_connection(|conn| dao::upsert_many_file_tags(conn, concat(file_tags_to_add)))
        }
    }
}

fn to_all_file_tags(files: Vec<File>, tags: &Vec<String>) -> Vec<FileTag> {
    files
        .into_iter()
        .cartesian_product(tags)
        .map(|(file, tag)| FileTag::new(file.id, tag.clone()))
        .collect::<Vec<FileTag>>()
}
