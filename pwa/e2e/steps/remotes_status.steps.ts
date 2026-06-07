import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const CONNECT_ERROR = 'Could not connect to the server. Check the URL and credentials.';

Given(
	'a read-flow server is running with user {string} and passphrase {string}',
	async function (this: BddWorld, user: string, passphrase: string) {
		expect(user).toBe(this.fixtures.backend.user);
		expect(passphrase).toBe(this.fixtures.backend.password);
		expect(this.fixtures.backend.baseUrl).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
	},
);

// The PWA's "add remote source" maps onto driving the real `/settings/sources`
// "Add source" form — REST/Cosmic drivers map the same Gherkin onto their own
// natural shapes (see features/remotes_status.feature).
When(
	'I add that server as a remote source named {string} with user {string} and passphrase {string}',
	async function (this: BddWorld, name: string, user: string, passphrase: string) {
		await this.page.goto(`${this.baseUrl}/settings/sources`);
		await this.page.getByRole('button', { name: 'Add source' }).click();
		await this.page.getByLabel('Name').fill(name);
		await this.page.getByLabel('Base URL').fill(this.fixtures.backend.baseUrl);
		await this.page.getByLabel('User ID').fill(user);
		await this.page.getByLabel('Passphrase').fill(passphrase);
		await this.page.getByRole('button', { name: 'Add', exact: true }).click();
	},
);

// The connectivity check verifies a PBKDF2 hash (600k iterations) server-side
// — noticeably slower than the default 5s web-first-assertion timeout.
const STATUS_CHECK_TIMEOUT = 15_000;

Then('the remote source {string} is reported as reachable', async function (this: BddWorld, name: string) {
	await expect(this.page.getByText(name, { exact: true })).toBeVisible({ timeout: STATUS_CHECK_TIMEOUT });
	await expect(this.page.getByText(CONNECT_ERROR)).toBeHidden();
});

Then('the remote source {string} is reported as unreachable', async function (this: BddWorld, name: string) {
	await expect(this.page.getByText(CONNECT_ERROR)).toBeVisible({ timeout: STATUS_CHECK_TIMEOUT });
	await expect(this.page.getByText(name, { exact: true })).toBeHidden();
});
