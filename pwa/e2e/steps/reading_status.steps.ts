import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_AUTH_TIMEOUT = 15_000;

async function goToDocumentDetail(world: BddWorld): Promise<void> {
	const fingerprint = world.currentDocumentFingerprint;
	if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
	await world.page.goto(`${world.baseUrl}/documents/${fingerprint}`);
	// Wait for the reading-status select to be interactable — indicates the document loaded.
	await expect(world.page.locator('select')).toBeEnabled({ timeout: SLOW_AUTH_TIMEOUT });
}

When(
	"I set the document's reading status to {string}",
	async function (this: BddWorld, status: string) {
		await goToDocumentDetail(this);
		await this.page.locator('select').selectOption(status);
		// Wait for the status update to complete — select becomes enabled again
		// once the async PUT /reading-state/<fp>/status roundtrip finishes.
		await expect(this.page.locator('select')).toBeEnabled({ timeout: SLOW_AUTH_TIMEOUT });
	},
);

Then(
	"the document's reading status is {string}",
	async function (this: BddWorld, status: string) {
		await expect(this.page.locator('select')).toHaveValue(status);
	},
);
