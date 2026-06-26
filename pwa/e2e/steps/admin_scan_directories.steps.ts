import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SOURCE_NAME = 'Home Server';

// Both the connectivity check (on add) and the admin page's data loads verify
// a PBKDF2 hash (600k iterations) server-side — noticeably slower than the
// default 5s web-first-assertion timeout (see `remotes_status`'s
// `STATUS_CHECK_TIMEOUT`).
const SLOW_AUTH_TIMEOUT = 15_000;

// The PWA's admin UI manages a *remote* instance's configuration through a
// registered source — same "register, then navigate" precondition
// `remotes_manage`/`admin_server_settings` document. REST/Cosmic manage their
// own booted backend's config directly, with no such indirection (see the
// feature's doc comment).
Given('I am viewing its scan directory configuration', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/settings/sources`);
	await this.page.getByRole('button', { name: 'Add source' }).click();
	await this.page.getByLabel('Name').fill(SOURCE_NAME);
	await this.page.getByLabel('Base URL').fill(this.fixtures.backend.baseUrl);
	await this.page.getByLabel('User ID').fill(this.fixtures.backend.user);
	await this.page.getByLabel('Passphrase').fill(this.fixtures.backend.password);
	await this.page.getByRole('button', { name: 'Add', exact: true }).click();
	await expect(this.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });

	await this.page.goto(`${this.baseUrl}/settings/admin`);
	await expect(this.page.getByPlaceholder('/absolute/path')).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});

// Scoped to the div containing the add-directory form — narrows the "Add"
// button match away from "Add source"/the authorized-users "Add" button that
// also live on this page.
function scanDirectoriesForm(world: BddWorld) {
	return world.page.locator('div', { has: world.page.getByPlaceholder('/absolute/path') }).last();
}

When('I add {string} as a scan directory', async function (this: BddWorld, path: string) {
	const form = scanDirectoriesForm(this);
	const button = form.getByRole('button', { name: 'Add', exact: true });
	await form.getByPlaceholder('/absolute/path').fill(path);
	await button.click();
	// `putScanDirectory` re-verifies the PBKDF2 hash server-side too — same
	// slow path as `admin_server_settings`'s save. Don't wait on the button's
	// enabled state: on success `addDir` clears the path field, which *also*
	// disables the button (`disabled={dirsBusy || !newPath.trim()}`) — it
	// never re-enables. The cleared field is itself the success signal.
	await expect(form.getByPlaceholder('/absolute/path')).toHaveValue('', { timeout: SLOW_AUTH_TIMEOUT });
});

Then('{string} appears in the list of scan directories', async function (this: BddWorld, path: string) {
	await expect(this.page.getByText(path, { exact: true })).toBeVisible();
});
