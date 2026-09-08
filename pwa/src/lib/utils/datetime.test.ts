import { describe, expect, it } from 'vitest';

import { formatTimestamp, parseTimestamp } from './datetime';

describe('parseTimestamp', () => {
	it('parses unix microseconds to an epoch-millisecond date', () => {
		const d = parseTimestamp('1767823456789000');
		expect(d?.getTime()).toBe(1767823456789);
	});

	it('parses rfc3339 strings', () => {
		const d = parseTimestamp('2026-09-08T09:00:00.000Z');
		expect(d?.getTime()).toBe(new Date('2026-09-08T09:00:00.000Z').getTime());
	});

	it('returns null for unparseable input', () => {
		expect(parseTimestamp('not-a-date')).toBeNull();
		expect(parseTimestamp('')).toBeNull();
	});
});

describe('formatTimestamp', () => {
	it('passes through values it cannot parse', () => {
		expect(formatTimestamp('not-a-date')).toBe('not-a-date');
	});

	it('does not emit the raw microsecond value for valid input', () => {
		expect(formatTimestamp('1767823456789000')).not.toBe('1767823456789000');
	});
});