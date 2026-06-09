import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 10_000;

When('I set the theme mode to {string}', async function (this: BddWorld, modeLabel: string) {
	await this.page.goto(`${this.baseUrl}/settings/theme`);
	await expect(this.page.getByRole('button', { name: modeLabel, exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
	await this.page.getByRole('button', { name: modeLabel, exact: true }).click();
});

Then('the theme mode {string} is persisted', async function (this: BddWorld, mode: string) {
	const savedMode = await this.page.evaluate(() => localStorage.getItem('read-flow-mode'));
	expect(savedMode).toBe(mode);
});
