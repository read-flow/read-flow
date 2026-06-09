import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SOURCE_NAME = 'Home Server';
const SLOW_AUTH_TIMEOUT = 15_000;
const CHECK_RESULT_TIMEOUT = 20_000;

async function ensureSourceRegistered(world: BddWorld): Promise<void> {
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

When('I run the check-missing operation', async function (this: BddWorld) {
	await ensureSourceRegistered(this);
	await this.page.goto(`${this.baseUrl}/settings/admin`);
	// Wait for the admin page to load fully (settings require PBKDF2 auth).
	await expect(this.page.getByRole('button', { name: 'Check', exact: true })).toBeEnabled({ timeout: SLOW_AUTH_TIMEOUT });
	await this.page.getByRole('button', { name: 'Check', exact: true }).click();
});

Then('no files are reported as missing', async function (this: BddWorld) {
	await expect(this.page.getByText('All files present.')).toBeVisible({ timeout: CHECK_RESULT_TIMEOUT });
});
