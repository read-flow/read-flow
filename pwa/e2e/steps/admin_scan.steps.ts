import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

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
const SLOW_AUTH_TIMEOUT = 15_000;
// Scan can take a moment once the PBKDF2 auth check completes.
const SCAN_RESULT_TIMEOUT = 20_000;

// The PWA triggers scans against a *registered* remote source — register it
// first, then navigate to the admin page to trigger the scan.
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

// Copies the fixture EPUB to a temp-like path the server can reach, registers
// it as a scan directory via the backend API directly (the scan-directory form
// is already covered by `admin_scan_directories`).
Given('a document is available in a configured scan directory', async function (this: BddWorld) {
	await registerBackendAsSource(this);

	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);

	// Upload the fixture so it exists on the server's filesystem.
	const form = new FormData();
	form.append('file', new Blob([readFileSync(FIXTURE_PATH)], { type: 'application/epub+zip' }), 'sample.epub');
	const uploadResponse = await fetch(`${baseUrl}/files`, {
		method: 'POST',
		headers: { Authorization: auth },
		body: form,
	});
	expect(uploadResponse.ok, `POST /files failed: ${uploadResponse.status}`).toBe(true);
	const file = (await uploadResponse.json()) as { guid: string; path: string };

	// Register the directory containing the uploaded file as a scan directory.
	const dir = file.path.substring(0, file.path.lastIndexOf('/'));
	await fetch(`${baseUrl}/scan-directories`, {
		method: 'PUT',
		headers: { Authorization: auth, 'Content-Type': 'application/json' },
		body: JSON.stringify({ path: dir, action: 'Scan', tags: [], inherit: false }),
	});
});

When('I trigger a library scan', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/settings/admin`);
	await expect(this.page.getByRole('button', { name: 'Scan', exact: true })).toBeEnabled({ timeout: SLOW_AUTH_TIMEOUT });
	await this.page.getByRole('button', { name: 'Scan', exact: true }).click();
});

Then('the scan reports at least 1 document processed', async function (this: BddWorld) {
	// The result line reads "Discovered N, processed N, errors N."
	await expect(this.page.getByText(/processed\s+[1-9]\d*/, { exact: false })).toBeVisible({ timeout: SCAN_RESULT_TIMEOUT });
});
