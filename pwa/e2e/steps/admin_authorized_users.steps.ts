import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SOURCE_NAME = 'Home Server';

// Both the connectivity check (on add) and the admin page's data loads verify
// a PBKDF2 hash (600k iterations) server-side — noticeably slower than the
// default 5s web-first-assertion timeout (see `remotes_status`'s
// `STATUS_CHECK_TIMEOUT`).
const SLOW_AUTH_TIMEOUT = 15_000;

// `POST /users` does *two* PBKDF2 round trips server-side — verifying the
// requesting owner's Basic-Auth credentials AND hashing the new user's
// password (600k iterations each) — measured at ~20s combined, comfortably
// over `SLOW_AUTH_TIMEOUT`.
const USER_CREATE_TIMEOUT = 30_000;

// The PWA's admin UI manages a *remote* instance's users through a registered
// source — same "register, then navigate" precondition `remotes_manage`/
// `admin_server_settings`/`admin_scan_directories` document. REST/Cosmic
// manage their own booted backend's users directly, with no such indirection
// (see the feature's doc comment).
Given('I am viewing its authorized users', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/settings/sources`);
	await this.page.getByRole('button', { name: 'Add source' }).click();
	await this.page.getByLabel('Name').fill(SOURCE_NAME);
	await this.page.getByLabel('Base URL').fill(this.fixtures.backend.baseUrl);
	await this.page.getByLabel('User ID').fill(this.fixtures.backend.user);
	await this.page.getByLabel('Passphrase').fill(this.fixtures.backend.password);
	await this.page.getByRole('button', { name: 'Add', exact: true }).click();
	await expect(this.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });

	await this.page.goto(`${this.baseUrl}/settings/admin`);
	await expect(this.page.getByPlaceholder('user id')).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});

// Scoped to the div containing the new-user form — narrows the "Add" button
// match away from "Add source" and the scan-directories "Add" button that
// also live on this page (see `admin_scan_directories.steps.ts`'s identical
// disambiguation rationale).
function newUserForm(world: BddWorld) {
	return world.page.locator('div', { has: world.page.getByPlaceholder('user id') }).last();
}

When(
	'I add a user {string} with passphrase {string}',
	async function (this: BddWorld, userId: string, password: string) {
		const form = newUserForm(this);
		const button = form.getByRole('button', { name: 'Add', exact: true });
		await form.getByPlaceholder('user id').fill(userId);
		await form.getByPlaceholder('password').fill(password);
		await button.click();
		// Don't wait on the button's enabled state: on success `addUser` clears
		// both fields, which *also* disables the button
		// (`disabled={usersBusy || !newUserId.trim() || !newUserPw}`) — it never
		// re-enables. The cleared field is itself the success signal (see
		// `USER_CREATE_TIMEOUT`'s doc comment for why this needs the longer wait).
		await expect(form.getByPlaceholder('user id')).toHaveValue('', { timeout: USER_CREATE_TIMEOUT });
	},
);

Then('{string} appears in the list of authorized users', async function (this: BddWorld, userId: string) {
	await expect(this.page.getByText(userId, { exact: true })).toBeVisible();
});
