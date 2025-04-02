use relm4::RelmApp;

use archive_organizer::ApplicationModule;
use archive_organizer_gtk::app::App;

fn main() -> anyhow::Result<()> {
    let app = RelmApp::new("net.kleinhaneveld.ArchiveOrganizer");
    let application_module = ApplicationModule::instantiate()?;
    let db_client = application_module.db_client();
    app.run_async::<App>(db_client.into());

    Ok(())
}
