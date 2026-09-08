// @feature: admin.activity_history
/**
 * Parse either an RFC-3339 string or a Unix-microseconds decimal string (the
 * format the activity store persists and the server returns for audit
 * timestamps) into a Date. Returns null when unparseable.
 */
export function parseTimestamp(value: string): Date | null {
	const d = /^\d+$/.test(value) ? new Date(Number(value) / 1000) : new Date(value);
	return Number.isNaN(d.getTime()) ? null : d;
}

/** Locale-formatted date-time string, or the raw value when unparseable. */
export function formatTimestamp(value: string): string {
	return parseTimestamp(value)?.toLocaleString() ?? value;
}