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
		// The source filter `<select>` only renders when at least 2 sources are
		// registered, and adding a source verifies connectivity — an unreachable
		// dummy is rejected. Register a second *real* source (same backend, other
		// name) purely so the filter becomes visible.
		await ensureSourceRegistered(this, 'BDD Second Source');
		await this.page.goto(`${this.baseUrl}/library`);
		const filterSelect = this.page.getByLabel('Filter by source');
		await expect(filterSelect).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
		// The PWA has no local library — "Local" (the COSMIC surface's own
		// collection) maps to the seeded backend source, "Home Server".
		const label = sourceName === 'Local' ? 'Home Server' : sourceName;
		await filterSelect.selectOption({ label });
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
