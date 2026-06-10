import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { Given, Then } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;
const SOURCE_NAME = 'BDD Backend';

const EPUB_FIXTURE_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..', '..', '..', 'features', 'fixtures', 'sample.epub',
);
const PDF_FIXTURE_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..', '..', '..', 'features', 'fixtures', 'sample.pdf',
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

async function uploadFile(
	world: BddWorld,
	filePath: string,
	filename: string,
	mimeType: string,
): Promise<{ guid: string; document_guid: string; fingerprint: string }> {
	const { baseUrl, user, password } = world.fixtures.backend;
	const auth = basicAuthHeader(user, password);
	const bytes = readFileSync(filePath);
	const form = new FormData();
	form.append('file', new Blob([bytes], { type: mimeType }), filename);
	const res = await fetch(`${baseUrl}/files`, {
		method: 'POST',
		headers: { Authorization: auth },
		body: form,
	});
	expect(res.ok, `POST /files (${filename}) failed: ${res.status}`).toBe(true);
	return res.json() as Promise<{ guid: string; document_guid: string; fingerprint: string }>;
}

async function mergeDocuments(world: BddWorld, winnerGuid: string, loserGuid: string): Promise<void> {
	const { baseUrl, user, password } = world.fixtures.backend;
	const auth = basicAuthHeader(user, password);
	const res = await fetch(`${baseUrl}/documents/merge`, {
		method: 'POST',
		headers: { Authorization: auth, 'Content-Type': 'application/json' },
		body: JSON.stringify({ winner_guid: winnerGuid, loser_guids: [loserGuid] }),
	});
	expect(res.ok, `POST /documents/merge failed: ${res.status}`).toBe(true);
}

// `Given a read-flow server is running…` is in common.steps.ts.

Given(
	'an EPUB and a PDF document have been added and merged',
	async function (this: BddWorld) {
		await ensureSourceRegistered(this);
		const epub = await uploadFile(this, EPUB_FIXTURE_PATH, 'sample.epub', 'application/epub+zip');
		const pdf = await uploadFile(this, PDF_FIXTURE_PATH, 'sample.pdf', 'application/pdf');
		await mergeDocuments(this, epub.document_guid, pdf.document_guid);
		this.currentDocumentApiGuid = epub.document_guid;
		this.currentDocumentFingerprint = epub.fingerprint;
	},
);

Then(
	'multiple format choices are available for the merged document',
	async function (this: BddWorld) {
		// Navigate to the document list and click the merged document row.
		// The format picker dialog should appear because the document has > 0 otherFormats.
		await this.page.goto(`${this.baseUrl}/`);
		// Wait for the list to load by looking for the EPUB title.
		const row = this.page.getByRole('link', { name: /BDD Sample Book/i }).first();
		await expect(row).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
		await row.click();
		// The format picker dialog shows when otherFormats.length > 0.
		const dialog = this.page.locator('[role="dialog"]').filter({ hasText: /BDD Sample Book/i });
		await expect(dialog).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
	},
);
