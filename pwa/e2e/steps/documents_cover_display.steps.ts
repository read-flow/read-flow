import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;
const SOURCE_NAME = 'BDD Backend';

const COVER_FIXTURE_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..', '..', '..', 'features', 'fixtures', 'sample_cover.epub',
);

function basicAuthHeader(user: string, password: string): string {
	return `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`;
}

async function ensureSourceRegistered(world: BddWorld): Promise<void> {
	await world.page.goto(`${world.baseUrl}/settings/sources`);
	if (await world.page.getByText(SOURCE_NAME, { exact: true }).isVisible()) return;
	await world.page.getByRole('button', { name: 'Add source' }).click();
	await world.page.getByLabel('Name').fill(SOURCE_NAME);
	await world.page.getByLabel('Base URL').fill(world.fixtures.backend.baseUrl);
	await world.page.getByLabel('User ID').fill(world.fixtures.backend.user);
	await world.page.getByLabel('Passphrase').fill(world.fixtures.backend.password);
	await world.page.getByRole('button', { name: 'Add', exact: true }).click();
	await expect(world.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
}

// `Given a read-flow server is running…` is in common.steps.ts.

Given(
	'a document with a cover image has been added to the library',
	async function (this: BddWorld) {
		await ensureSourceRegistered(this);

		const { baseUrl, user, password } = this.fixtures.backend;
		const auth = basicAuthHeader(user, password);
		const bytes = readFileSync(COVER_FIXTURE_PATH);
		const form = new FormData();
		form.append('file', new Blob([bytes], { type: 'application/epub+zip' }), 'sample_cover.epub');

		const res = await fetch(`${baseUrl}/files`, {
			method: 'POST',
			headers: { Authorization: auth },
			body: form,
		});
		expect(res.ok, `POST /files (sample_cover.epub) failed: ${res.status}`).toBe(true);
		const json = (await res.json()) as { guid: string; document_guid: string; fingerprint: string };
		this.currentDocumentGuid = json.guid;
		this.currentDocumentApiGuid = json.document_guid;
		this.currentDocumentFingerprint = json.fingerprint;
	},
);

When("I request the document's cover", async function (this: BddWorld) {
	const fp = this.currentDocumentFingerprint ?? this.currentDocumentGuid ?? '';

	// Cover extraction runs asynchronously after upload; the page only renders
	// a CoverImage when the fetched file already has has_cover=true, and it
	// doesn't re-poll. Wait server-side for extraction to finish first.
	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);
	const deadline = Date.now() + SLOW_LOAD_TIMEOUT;
	for (;;) {
		const res = await fetch(`${baseUrl}/files`, { headers: { Authorization: auth } });
		const files = (await res.json()) as Array<{ fingerprint: string; has_cover?: boolean }>;
		if (files.some((f) => f.fingerprint === fp && f.has_cover)) break;
		if (Date.now() > deadline) throw new Error(`cover for ${fp} was never extracted`);
		await new Promise((resolve) => setTimeout(resolve, 200));
	}

	// Navigate to the document detail page where the cover is displayed.
	await this.page.goto(`${this.baseUrl}/documents/${fp}`);
	await expect(this.page.getByRole('heading', { level: 1 })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
});

Then('a cover image is returned', async function (this: BddWorld) {
	// DocumentDetail renders a CoverImage (<img class="…object-cover…">) when
	// has_cover is true — the hero cover first, then per-format thumbnails, so
	// take the first match.
	const coverImg = this.page.locator('img.object-cover').first();
	await expect(coverImg).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
});
