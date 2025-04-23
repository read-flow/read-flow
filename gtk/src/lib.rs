pub mod app;
// We'll implement this in a future update
// pub mod duplicates_dialog;
pub mod duplicates_page;
pub mod file_box;
pub mod file_details;
pub mod file_details_section;
pub mod file_info_section;
pub mod file_list;
pub mod settings_dialog;
pub mod status_radio_group;
pub mod tag_badge;
pub mod tag_input;
pub mod ui_utils;

use url::Url;

use archive_organizer::{ApplicationModule, client::FilesClient, db::dao::RemoteDao};

pub fn get_remote_clients(
    application_module: &ApplicationModule,
) -> anyhow::Result<Vec<FilesClient>> {
    application_module
        .connection_pool
        .select_all_remotes()?
        .into_iter()
        .map(|remote| {
            let remote_connection: Url = remote.base_url.parse()?;
            let client = FilesClient::new(remote_connection.clone())?;
            Ok(client)
        })
        .collect::<anyhow::Result<_>>()
}
