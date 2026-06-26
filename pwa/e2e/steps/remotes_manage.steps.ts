import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SOURCE_NAME = 'Home Server';

// `register` reuses the real "Add source" form — same plumbing as
// `remotes_status`'s "add that server as a remote source" When step, just
// invoked as a `Given` precondition here.
Given(
	'that server is registered as a remote source with user {string} and passphrase {string}',
	async function (this: BddWorld, user: string, passphrase: string) {
		await this.page.goto(`${this.baseUrl}/settings/sources`);
		await this.page.getByRole('button', { name: 'Add source' }).click();
		await this.page.getByLabel('Name').fill(SOURCE_NAME);
		await this.page.getByLabel('Base URL').fill(this.fixtures.backend.baseUrl);
		await this.page.getByLabel('User ID').fill(user);
		await this.page.getByLabel('Passphrase').fill(passphrase);
		await this.page.getByRole('button', { name: 'Add', exact: true }).click();
		await expect(this.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({ timeout: 15_000 });
	},
);

When('I remove that remote source', async function (this: BddWorld) {
	this.page.once('dialog', (dialog) => dialog.accept());
	await this.page.getByRole('button', { name: 'Remove source' }).click();
});

Then('the list of remote sources is empty', async function (this: BddWorld) {
	await expect(this.page.getByText('No sources yet.')).toBeVisible();
	await expect(this.page.getByText(SOURCE_NAME, { exact: true })).toBeHidden();
});
