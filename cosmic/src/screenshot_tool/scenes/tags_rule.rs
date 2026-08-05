use std::path::Path;

use cosmic_golden::HeadlessRenderer;
use read_flow_core::ExpandedPath;
use read_flow_core::scan::DirectorySettings;

use crate::forms::settings::directory_settings::DirectorySettingsForm;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (_application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    let path = ExpandedPath::try_from(sample_library.to_path_buf())
        .map_err(|e| anyhow::anyhow!("expand sample library path: {e}"))?;
    let settings = DirectorySettings::Scan {
        tags: vec!["classics".to_string(), "public-domain".to_string()],
        inherit: false,
    };

    let (form, init_task) = DirectorySettingsForm::new(Some((path, settings)), document_provider);
    crate::test_support::drain(init_task).await;

    let element = form.view();
    let mut renderer = HeadlessRenderer::with_theme(super::super::theme(sample_library)?);
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
}
