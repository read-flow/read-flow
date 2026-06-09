import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_AUTH_TIMEOUT = 15_000;

async function goToDocumentDetail(world: BddWorld): Promise<void> {
	const fingerprint = world.currentDocumentFingerprint;
	if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
	await world.page.goto(`${world.baseUrl}/documents/${fingerprint}`);
	// Wait for the tag input — confirms the document has loaded and metadata is rendered.
	await expect(world.page.getByPlaceholder('Add a tag…')).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
}

When("I view the document's details", async function (this: BddWorld) {
	await goToDocumentDetail(this);
});

Then("the document's title is {string}", async function (this: BddWorld, title: string) {
	await expect(this.page.getByRole('heading', { name: title, exact: true })).toBeVisible();
});
