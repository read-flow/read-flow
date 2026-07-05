import { When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;

// "Then … appears in the filtered results" is defined in
// documents_filter_by_status.steps.ts and shared with this feature.
When('I filter by tag {string}', async function (this: BddWorld, tag: string) {
	await this.page.goto(`${this.baseUrl}/library`);
	// Wait for the tag filter chip to appear (only once documents have loaded
	// and the aggregator has returned tags).
	await expect(this.page.getByRole('button', { name: tag, exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
	// Click the chip to add it to the allow-filter.
	await this.page.getByRole('button', { name: tag, exact: true }).click();
});
