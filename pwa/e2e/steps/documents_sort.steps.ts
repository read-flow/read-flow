import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const FIXTURE2_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
	'..',
	'..',
	'features',
	'fixtures',
	'sample2.epub',
);

const SOURCE_NAME = 'Home Server';
const SLOW_LOAD_TIMEOUT = 15_000;

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

Given('two documents have been added to the library', async function (this: BddWorld) {
	await ensureSourceRegistered(this);

	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);

	for (const [filename, bytes] of [
		['sample.epub', readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..', 'features', 'fixtures', 'sample.epub'))],
		['sample2.epub', readFileSync(FIXTURE2_PATH)],
	] as [string, Buffer][]) {
		const form = new FormData();
		form.append('file', new Blob([bytes], { type: 'application/epub+zip' }), filename);
		const res = await fetch(`${baseUrl}/files`, {
			method: 'POST',
			headers: { Authorization: auth },
			body: form,
		});
		expect(res.ok, `POST /files (${filename}) failed: ${res.status}`).toBe(true);
	}
});

When('I sort the documents by title ascending', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/library`);
	// Wait for documents to appear.
	await expect(this.page.getByText('BDD Sample Book', { exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
	// Select "Title" in the sort-by dropdown.
	await this.page.getByLabel('Sort by').selectOption('title');
	// Ensure direction is ascending — the button title says "Ascending" when asc.
	const dirBtn = this.page.getByLabel('Toggle sort direction');
	const title = await dirBtn.getAttribute('title');
	if (title !== 'Ascending') {
		await dirBtn.click();
		await expect(dirBtn).toHaveAttribute('title', 'Ascending');
	}
});

Then(
	'{string} appears before {string} in the list',
	async function (this: BddWorld, first: string, second: string) {
		// Compare vertical position of the two title elements.
		const box1 = await this.page.getByText(first, { exact: true }).first().boundingBox();
		const box2 = await this.page.getByText(second, { exact: true }).first().boundingBox();
		expect(box1, `${first} not found`).not.toBeNull();
		expect(box2, `${second} not found`).not.toBeNull();
		expect(box1!.y, `expected ${first} (y=${box1!.y}) before ${second} (y=${box2!.y})`).toBeLessThan(
			box2!.y,
		);
	},
);
