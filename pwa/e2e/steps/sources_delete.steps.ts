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

// The seeded file — earlier scenarios may have attached other formats (e.g.
// sample.pdf via the format-picker scenario) to the same document, so target
// this format's row rather than "the only delete button".
const SEEDED_FILE_NAME = 'sample.epub';

function seededFormatRow(world: BddWorld) {
	return world.page.getByRole('listitem').filter({ hasText: SEEDED_FILE_NAME });
}

When('I delete the document', async function (this: BddWorld) {
	await goToDocumentDetail(this);
	// Open the formats management panel.
	await this.page.getByRole('button', { name: 'Manage', exact: true }).click();
	// Click the trash icon in the seeded format's row (aria-label="Delete this format").
	const row = seededFormatRow(this);
	await row.getByRole('button', { name: 'Delete this format' }).click();
	// Confirm deletion in the inline confirmation.
	await row.getByRole('button', { name: 'Delete', exact: true }).click();
	// Wait for the deletion to complete — the format's row disappears.
	await expect(row).not.toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
});

Then('the file no longer appears in the file index', async function (this: BddWorld) {
	// After deletion the seeded format's row is gone from the formats list.
	await expect(seededFormatRow(this)).not.toBeVisible();
});
