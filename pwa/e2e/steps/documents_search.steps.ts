import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;

When('I search for {string}', async function (this: BddWorld, query: string) {
	await this.page.goto(`${this.baseUrl}/library`);
	// Wait for the document list to finish loading before typing (avoids race
	// where the search input is not yet interactive).
	await expect(this.page.getByPlaceholder('Search documents…')).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
	await this.page.getByPlaceholder('Search documents…').fill(query);
});

Then('{string} appears in the search results', async function (this: BddWorld, title: string) {
	await expect(this.page.getByText(title, { exact: true })).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
});
