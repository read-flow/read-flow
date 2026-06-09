import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SOURCE_NAME = 'Home Server';
const SLOW_AUTH_TIMEOUT = 15_000;

// The PWA stores `privateMode` per-source in IndexedDB; the client then sends
// `X-Private-Mode: true` on every request to that source. "Enabling private
// mode" here means adding the source with the privateMode checkbox checked —
// the toggle on an existing source row uses a different code path (covered by
// the sources.manage feature) but the observable outcome is the same.
When('I enable private mode', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/settings/sources`);
	// Add the source with privateMode checked if it is not yet registered.
	if (!await this.page.getByText(SOURCE_NAME, { exact: true }).isVisible()) {
		await this.page.getByRole('button', { name: 'Add source' }).click();
		await this.page.getByLabel('Name').fill(SOURCE_NAME);
		await this.page.getByLabel('Base URL').fill(this.fixtures.backend.baseUrl);
		await this.page.getByLabel('User ID').fill(this.fixtures.backend.user);
		await this.page.getByLabel('Passphrase').fill(this.fixtures.backend.password);
		await this.page.getByLabel('Private mode').check();
		await this.page.getByRole('button', { name: 'Add', exact: true }).click();
		await expect(this.page.getByText(SOURCE_NAME, { exact: true })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
	} else {
		// Source already registered — enable via the lock toggle if not already on.
		const offToggle = this.page.getByTitle('Private mode off — click to enable');
		if (await offToggle.isVisible()) {
			await offToggle.click();
		}
	}
});

Then('private mode is reported as enabled', async function (this: BddWorld) {
	await expect(
		this.page.getByTitle('Private mode on — click to disable'),
	).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});
