pub mod app;
pub mod file_box;
pub mod file_details;

use url::Url;

use archive_organizer::{ApplicationModule, client::FilesClient, db::dao::RemoteDao};

pub fn get_remote_clients(application_module: &ApplicationModule) -> anyhow::Result<Vec<FilesClient>> {
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
