import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;

When('I filter by reading status {string}', async function (this: BddWorld, status: string) {
	await this.page.goto(`${this.baseUrl}/library`);
	// Wait for the document list to be ready.
	await expect(this.page.getByLabel('Filter by reading status')).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
	await this.page.getByLabel('Filter by reading status').selectOption(status);
});

// "… appears in the filtered results" — shared with documents_filter_by_tag
Then('{string} appears in the filtered results', async function (this: BddWorld, title: string) {
	await expect(this.page.getByText(title, { exact: true })).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
});
