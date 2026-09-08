// @feature: admin.activity_history
// @feature: documents.activity_history
//
// Wire types for the activity-history endpoints exposed by `GET /activity`,
// `GET /activity/{id}`, and `GET /documents/{guid}/activity`. The shapes mirror
// `read-flow-core`'s `crate::activity` module so the PWA renders the same
// structured audit trail as the COSMIC page. Localized presentation happens at
// render time; the wire carries only codes and typed parameters.
//
// The actual fetch methods live on `ReadFlowClient` (`getActivity`,
// `getActivityDetail`, `getDocumentActivity`); this module carries the shared
// types only.

/** Who performed an operation. Never selectable by a request body — the server
 * derives it from the authenticated session (or marks it local/system). */
export interface ActivityActor {
	kind: 'user' | 'local_identity' | 'local_session' | 'system';
	id: string | null;
}

export type ActivityChannel = 'rest' | 'cosmic' | 'cli' | 'internal';

export type OperationType =
	| 'scan'
	| 'file_delete'
	| 'document_merge'
	| 'metadata_edit'
	| 'tag_edit'
	| 'status_edit'
	| 'cover_edit'
	| 'missing_file_maintenance';

export type OperationStatus = 'started' | 'completed' | 'failed' | 'interrupted';

export type AuditOutcome =
	| 'success'
	| 'completed_with_errors'
	| 'failed'
	| 'observed'
	| 'proposed';

export interface ActivityOperation {
	id: string;
	operation_type: OperationType;
	actor: ActivityActor;
	channel: ActivityChannel;
	status: OperationStatus;
	dry_run: boolean;
	started_at: string;
	completed_at: string | null;
	error_code: string | null;
	counts: Record<string, unknown>;
}

export interface ActivityTarget {
	target_kind: string;
	target_id: string;
	title_snapshot: string | null;
	path_snapshot: string | null;
	fingerprint_snapshot: string | null;
}

export interface ActivityEvent {
	id: number;
	operation_id: string;
	event_type: string;
	schema_version: number;
	sequence: number;
	occurred_at: string;
	outcome: AuditOutcome;
	parameters: Record<string, unknown>;
	targets: ActivityTarget[];
}

/** One operation with its ordered child events. The wire flattens the operation
 * fields onto the top level alongside `events` (see the server's
 * `ActivityDetailResponse` with `#[serde(flatten)]`). */
export interface ActivityDetail extends ActivityOperation {
	events: ActivityEvent[];
}

/** A page of operations, newest first, plus an optional cursor for the next page. */
export interface ActivityPage {
	operations: ActivityOperation[];
	next_cursor: { started_at: string; operation_id: string } | null;
}
