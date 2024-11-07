use itertools::{concat, Itertools};

use crate::{
    db::{
        dao::{Error, FileDao, FileTagDao},
        models::{File, FileTag},
    },
    scan::ScanSettings,
    ApplicationModule,
};

impl ApplicationModule {
    pub fn apply_tags(&self) -> Result<(), Error> {
        match &self.settings.scan {
            None => Ok(()),
            Some(scan_settings) => self.apply_tags_from_settings(scan_settings),
        }
    }

    fn apply_tags_from_settings(&self, scan_settings: &ScanSettings) -> Result<(), Error> {
        let file_tags_to_add = scan_settings
            .auto_tags
            .iter()
            .map(|(path, tags)| {
                self.connection_pool
                    .select_all_files_by_path_like(path)
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
            self.connection_pool
                .upsert_many_file_tags(concat(file_tags_to_add))
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
