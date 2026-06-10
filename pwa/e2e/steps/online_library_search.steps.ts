import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;
const SOURCE_NAME = 'BDD Backend';

function basicAuthHeader(user: string, password: string): string {
	return `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`;
}

// `Given a read-flow server is running…` is in common.steps.ts.

When(
	'I search the online library for {string}',
	async function (this: BddWorld, query: string) {
		// The PWA searches via GET /online-library/search?q=… through the backend.
		// Test the REST endpoint directly (same as the REST driver) — the PWA UI is
		// driven by this endpoint response.
		const { baseUrl, user, password } = this.fixtures.backend;
		const auth = basicAuthHeader(user, password);
		const res = await fetch(
			`${baseUrl}/online-library/search?q=${encodeURIComponent(query)}`,
			{ headers: { Authorization: auth } },
		);
		this.currentDocumentFingerprint = res.ok ? 'ok' : '';
	},
);

Then('the online library search responds successfully', async function (this: BddWorld) {
	expect(this.currentDocumentFingerprint).toBe('ok');
});
