import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_AUTH_TIMEOUT = 15_000;

// Navigates to the document detail page and waits for the document to load.
// The page calls refreshDocuments() which fetches from registered sources —
// the source must be registered in the preceding Given step.
async function goToDocumentDetail(world: BddWorld): Promise<void> {
	const fingerprint = world.currentDocumentFingerprint;
	if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
	await world.page.goto(`${world.baseUrl}/documents/${fingerprint}`);
	// Wait for the tag input to be visible — indicates the document has loaded.
	await expect(world.page.getByPlaceholder('Add a tag…')).toBeVisible({
		timeout: SLOW_AUTH_TIMEOUT,
	});
}

When('I add the tag {string} to the document', async function (this: BddWorld, tag: string) {
	await goToDocumentDetail(this);
	await this.page.getByPlaceholder('Add a tag…').fill(tag);
	await this.page.getByRole('button', { name: 'Add', exact: true }).click();
	// Wait for the tag to appear before returning so the Then step can assert immediately.
	await expect(this.page.getByText(tag, { exact: true })).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});

Then("{string} appears in the document's tag list", async function (this: BddWorld, tag: string) {
	await expect(this.page.getByText(tag, { exact: true })).toBeVisible();
});
