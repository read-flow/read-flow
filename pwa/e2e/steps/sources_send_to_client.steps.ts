import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;
const EPUB_FIXTURE_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..', '..', '..', 'features', 'fixtures', 'sample.epub',
);

function basicAuthHeader(user: string, password: string): string {
	return `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`;
}

// `Given a read-flow server is running…` is in common.steps.ts.

// The PWA sends a document to a source server via the `sendFileToSource` helper
// (aggregator.ts), which downloads from one source and uploads to another.
// In the BDD harness the "target" is the driver's own backend — we upload
// sample.epub directly via POST /files, which exercises the same REST path.
When('I send a document to the server', async function (this: BddWorld) {
	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);
	const bytes = readFileSync(EPUB_FIXTURE_PATH);
	const form = new FormData();
	form.append('file', new Blob([bytes], { type: 'application/epub+zip' }), 'sample.epub');
	const res = await fetch(`${baseUrl}/files`, {
		method: 'POST',
		headers: { Authorization: auth },
		body: form,
	});
	this.currentDocumentFingerprint = res.ok ? 'ok' : '';
	expect(res.ok, `POST /files failed: ${res.status}`).toBe(true);
});

Then('the document was accepted by the server', async function (this: BddWorld) {
	expect(this.currentDocumentFingerprint).toBe('ok');
});
