import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;
const PDF_LOAD_TIMEOUT = 30_000;
const SOURCE_NAME = 'BDD Backend';

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

// `Given a read-flow server is running…` is in common.steps.ts.

Given('a PDF document has been added to the library', async function (this: BddWorld) {
	await ensureSourceRegistered(this);

	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);
	const bytes = readFileSync(PDF_FIXTURE_PATH);
	const form = new FormData();
	form.append('file', new Blob([bytes], { type: 'application/pdf' }), 'sample.pdf');

	const res = await fetch(`${baseUrl}/files`, {
		method: 'POST',
		headers: { Authorization: auth },
		body: form,
	});
	expect(res.ok, `POST /files (sample.pdf) failed: ${res.status}`).toBe(true);
	const json = (await res.json()) as { guid: string; document_guid: string; fingerprint: string };
	this.currentDocumentGuid = json.guid;
	this.currentDocumentApiGuid = json.document_guid;
	this.currentDocumentFingerprint = json.fingerprint;
});

When('I open the PDF document for reading', async function (this: BddWorld) {
	const fp = this.currentDocumentFingerprint ?? '';
	await this.page.goto(`${this.baseUrl}/read/pdf/${fp}`);
});

Then('the PDF pages are displayed', async function (this: BddWorld) {
	// PDF.js renders pages inside a <canvas> element. Wait for at least one canvas.
	await expect(this.page.locator('canvas').first()).toBeVisible({ timeout: PDF_LOAD_TIMEOUT });
});
