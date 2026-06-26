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

async function ensureSourceRegistered(world: BddWorld, name: string): Promise<void> {
	await world.page.goto(`${world.baseUrl}/settings/sources`);
	if (await world.page.getByText(name, { exact: true }).isVisible()) return;
	await world.page.getByRole('button', { name: 'Add source' }).click();
	await world.page.getByLabel('Name').fill(name);
	await world.page.getByLabel('Base URL').fill(world.fixtures.backend.baseUrl);
	await world.page.getByLabel('User ID').fill(world.fixtures.backend.user);
	await world.page.getByLabel('Passphrase').fill(world.fixtures.backend.password);
	await world.page.getByRole('button', { name: 'Add', exact: true }).click();
	await expect(world.page.getByText(name, { exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
}

// `Given a read-flow server is running…` is in common.steps.ts.
// `And a document has been added to the library` is in documents_list.steps.ts.

When(
	'I filter documents by source {string}',
	async function (this: BddWorld, sourceName: string) {
		// The source filter `<select>` only renders when at least 2 sources are registered.
		// Register a second dummy source so the filter becomes visible, then select by label.
		const DUMMY_SOURCE = 'BDD Source Filter Dummy';
		await ensureSourceRegistered(this, 'BDD Backend');
		await this.page.goto(`${this.baseUrl}/settings/sources`);
		if (!(await this.page.getByText(DUMMY_SOURCE, { exact: true }).isVisible())) {
			await this.page.getByRole('button', { name: 'Add source' }).click();
			await this.page.getByLabel('Name').fill(DUMMY_SOURCE);
			await this.page.getByLabel('Base URL').fill('http://localhost:1');
			await this.page.getByLabel('User ID').fill('dummy');
			await this.page.getByLabel('Passphrase').fill('dummy');
			await this.page.getByRole('button', { name: 'Add', exact: true }).click();
			await expect(this.page.getByText(DUMMY_SOURCE, { exact: true })).toBeVisible({
				timeout: SLOW_LOAD_TIMEOUT,
			});
		}
		await this.page.goto(`${this.baseUrl}/`);
		const filterSelect = this.page.getByLabel('Filter by source');
		await expect(filterSelect).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
		await filterSelect.selectOption({ label: sourceName });
	},
);

Then(
	'{string} appears in the filtered document list',
	async function (this: BddWorld, title: string) {
		await expect(this.page.getByText(title, { exact: true })).toBeVisible({
			timeout: SLOW_LOAD_TIMEOUT,
		});
	},
);
