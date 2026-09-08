// SPDX-License-Identifier: AGPL-3.0-or-later

use itertools::Itertools;
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
        let user_id = settings.server.resolve_local_user_id().to_string();
        self.apply_tags_from_settings(&settings.scan, &user_id)
            .await
    }

    async fn apply_tags_from_settings(
        &self,
        scan_settings: &ScanSettings,
        user_id: &str,
    ) -> Result<(), Error> {
        let pool = self.connection_pool().await;
        let dry_run = scan_settings.dry_run;
        let context = self
            .local_audit_context(crate::audit::AuditChannel::Cli)
            .await;
        if let Err(error) = dao::create_audit_operation(
            &pool,
            &context,
            crate::audit::OperationType::TagEdit,
            dry_run,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start auto-tag audit operation: {error}");
        }

        let mut conn = pool.acquire().await?;
        let mut applied: u64 = 0;
        for (path, tags) in &scan_settings.auto_tags {
            let files = dao::select_all_files_by_path_like(&mut conn, user_id, path).await?;
            let count = files.len() as u64;
            if dry_run {
                for file in files.iter() {
                    println!("{}: {:?}", file.path, tags);
                }
            } else {
                dao::upsert_many_content_tags(&mut conn, to_all_content_tags(files, tags)).await?;
            }
            applied += count;
            let _ = dao::append_audit_event(
                &mut conn,
                &context.operation_id,
                crate::audit::AuditEventType::DocumentTagsAdded,
                if dry_run {
                    crate::audit::AuditOutcome::Proposed
                } else {
                    crate::audit::AuditOutcome::Success
                },
                &crate::audit::now_timestamp(),
                serde_json::json!({
                    "auto_tag_pattern": path,
                    "tags": tags,
                    "files": count,
                    "dry_run": dry_run,
                }),
                vec![],
            )
            .await
            .map_err(|e| {
                tracing::warn!("could not record auto-tag audit event: {e}");
                e
            });
        }

        dao::finish_audit_operation(
            &pool,
            &context.operation_id,
            crate::audit::OperationStatus::Completed,
            &crate::audit::now_timestamp(),
            None,
            serde_json::json!({ "tags_applied": applied }),
        )
        .await?;
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
