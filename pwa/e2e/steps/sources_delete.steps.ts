import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_AUTH_TIMEOUT = 15_000;

// Navigate to the document detail page (fingerprint was stored by the seed step).
async function goToDocumentDetail(world: BddWorld): Promise<void> {
	const fingerprint = world.currentDocumentFingerprint;
	if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
	await world.page.goto(`${world.baseUrl}/documents/${fingerprint}`);
	await expect(world.page.getByPlaceholder('Add a tag…')).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
}

When('I delete the document', async function (this: BddWorld) {
	await goToDocumentDetail(this);
	// Open the formats management panel.
	await this.page.getByRole('button', { name: 'Manage', exact: true }).click();
	// Click the trash icon for the format (aria-label="Delete this format").
	await this.page.getByRole('button', { name: 'Delete this format' }).click();
	// Confirm deletion in the inline confirmation.
	await this.page.getByRole('button', { name: 'Delete', exact: true }).click();
	// Wait for the deletion to complete — the "Manage" button returns to view.
	await expect(this.page.getByRole('button', { name: 'Manage', exact: true })).toBeVisible({
		timeout: SLOW_AUTH_TIMEOUT,
	});
});

Then('the file no longer appears in the file index', async function (this: BddWorld) {
	// After deletion the formats list is empty — the "Delete this format" trash
	// icon is gone. We are still in manage mode (manageFormats=true), so "Done"
	// is shown and delete buttons would be visible if any formats remained.
	await expect(this.page.getByRole('button', { name: 'Delete this format' })).not.toBeVisible();
});
