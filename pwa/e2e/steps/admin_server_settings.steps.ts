import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SOURCE_NAME = 'Home Server';

// Both the connectivity check (on add) and `getSettings()` verify a PBKDF2
// hash (600k iterations) server-side — noticeably slower than the default 5s
// web-first-assertion timeout (see `remotes_status`'s `STATUS_CHECK_TIMEOUT`).
const SLOW_AUTH_TIMEOUT = 15_000;

// The PWA's admin UI manages a *remote* instance's settings through a
// registered source — same "register, then navigate" precondition
// `remotes_manage` documents. REST/Cosmic manage their own booted backend's
// settings directly, with no such indirection (see the feature's doc comment).
Given('I am viewing its server settings', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/settings/sources`);
	await this.page.getByRole('button', { name: 'Add source' }).click();
	await this.page.getByLabel('Name').fill(SOURCE_NAME);
	await this.page.getByLabel('Base URL').fill(this.fixtures.backend.baseUrl);
	await this.page.getByLabel('User ID').fill(this.fixtures.backend.user);
	await this.page.getByLabel('Passphrase').fill(this.fixtures.backend.password);
	await this.page.getByRole('button', { name: 'Add', exact: true }).click();
	await expect(this.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });

	await this.page.goto(`${this.baseUrl}/settings/admin`);
	await expect(this.page.getByLabel('Dry run')).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});

When('I enable dry-run mode and save', async function (this: BddWorld) {
	await this.page.getByLabel('Dry run').check();
	await this.page.getByRole('button', { name: 'Save settings' }).click();
	// `putSettings` re-verifies the PBKDF2 hash server-side too — same slow path.
	await expect(this.page.getByRole('button', { name: 'Save settings' })).toBeEnabled({
		timeout: SLOW_AUTH_TIMEOUT,
	});
});

Then('dry-run mode is reported as enabled', async function (this: BddWorld) {
	await this.page.reload();
	await expect(this.page.getByLabel('Dry run')).toBeChecked({ timeout: SLOW_AUTH_TIMEOUT });
});
