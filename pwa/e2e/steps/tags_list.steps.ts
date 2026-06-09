import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Given, Then } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

// Shared with the Rust drivers — see `features/tags_list.feature`'s doc
// comment for why a real, parseable EPUB is required (the scanner only
// creates a `Document` row once metadata extraction succeeds).
const FIXTURE_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'..',
	'..',
	'features',
	'fixtures',
	'sample.epub',
);

function basicAuthHeader(user: string, password: string): string {
	return `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`;
}

const SOURCE_NAME = 'Home Server';

// Adding a source verifies connectivity (a PBKDF2 hash check, 600k
// iterations) server-side — see `remotes_status`'s `STATUS_CHECK_TIMEOUT` /
// `admin_*` steps' `SLOW_AUTH_TIMEOUT` for the same noticeably-slower-than-
// the-default-5s pattern.
const SLOW_AUTH_TIMEOUT = 15_000;

// The PWA only knows about documents served by *registered* remote sources —
// "added to the library" means both seeding the document on the backend AND
// registering that backend as a source so the PWA's aggregator picks it up.
// Same registration form `admin_scan_directories`/`admin_authorized_users`
// drive (`/settings/sources` "Add source").
async function registerBackendAsSource(world: BddWorld) {
	await world.page.goto(`${world.baseUrl}/settings/sources`);
	if (await world.page.getByText(SOURCE_NAME, { exact: true }).isVisible()) return;
	await world.page.getByRole('button', { name: 'Add source' }).click();
	await world.page.getByLabel('Name').fill(SOURCE_NAME);
	await world.page.getByLabel('Base URL').fill(world.fixtures.backend.baseUrl);
	await world.page.getByLabel('User ID').fill(world.fixtures.backend.user);
	await world.page.getByLabel('Passphrase').fill(world.fixtures.backend.password);
	await world.page.getByRole('button', { name: 'Add', exact: true }).click();
	await expect(world.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
}

// Seeded directly against the booted backend over HTTP — *not* through the
// browser/UI (mirrors `RestDriver::seed_tagged_document`'s `POST /files` +
// `POST /files/<guid>/tags`; `TestServer`/this backend exposes only HTTP, no
// DB access, so an upload-and-tag round trip is the only seeding path).
Given('a document tagged {string} has been added to the library', async function (this: BddWorld, tag: string) {
	await registerBackendAsSource(this);

	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);

	const form = new FormData();
	form.append('file', new Blob([readFileSync(FIXTURE_PATH)], { type: 'application/epub+zip' }), 'sample.epub');

	const uploadResponse = await fetch(`${baseUrl}/files`, {
		method: 'POST',
		headers: { Authorization: auth },
		body: form,
	});
	expect(uploadResponse.ok, `POST /files failed: ${uploadResponse.status}`).toBe(true);
	const file = (await uploadResponse.json()) as { guid: string; fingerprint: string };
	this.currentDocumentGuid = file.guid;
	this.currentDocumentFingerprint = file.fingerprint;

	const tagsResponse = await fetch(`${baseUrl}/files/${file.guid}/tags`, {
		method: 'POST',
		headers: { Authorization: auth, 'Content-Type': 'application/json' },
		body: JSON.stringify([tag]),
	});
	expect(tagsResponse.ok, `POST /files/<guid>/tags failed: ${tagsResponse.status}`).toBe(true);
});

Then("{string} appears in the library's list of tags", async function (this: BddWorld, tag: string) {
	await this.page.goto(`${this.baseUrl}/`);
	// The aggregator fetches documents from the newly-registered source
	// asynchronously after navigation — same "give it room to load" margin as
	// the slow-auth waits above, not a fixed UI animation.
	await expect(this.page.getByRole('button', { name: tag, exact: false })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});
