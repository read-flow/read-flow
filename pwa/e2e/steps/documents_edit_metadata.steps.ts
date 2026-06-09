import { When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_AUTH_TIMEOUT = 15_000;

async function goToDocumentDetail(world: BddWorld): Promise<void> {
	const fingerprint = world.currentDocumentFingerprint;
	if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
	await world.page.goto(`${world.baseUrl}/documents/${fingerprint}`);
	await expect(world.page.getByPlaceholder('Add a tag…')).toBeVisible({ timeout: SLOW_AUTH_TIMEOUT });
}

When("I set the document's title to {string}", async function (this: BddWorld, title: string) {
	await goToDocumentDetail(this);
	// Enter editing mode via the "Edit document info" icon button.
	await this.page.getByRole('button', { name: 'Edit document info' }).click();
	// Fill the title field, then save.
	await this.page.getByLabel('Title').fill(title);
	await this.page.getByRole('button', { name: 'Save', exact: true }).click();
	// Wait for edit mode to close (Save/Cancel buttons disappear) so the Then
	// step asserts against the persisted, non-editing view.
	await expect(this.page.getByRole('button', { name: 'Edit document info' })).toBeVisible({
		timeout: SLOW_AUTH_TIMEOUT,
	});
});

// "the document's title is {string}" is defined in documents_detail_view.steps.ts
