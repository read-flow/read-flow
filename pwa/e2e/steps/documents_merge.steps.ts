import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

// `Given two documents have been added to the library` is in documents_sort.steps.ts.

const SLOW_LOAD_TIMEOUT = 15_000;
const MERGE_TIMEOUT = 20_000;

When('I merge the two documents', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/`);

	// Wait for both documents to appear.
	await expect(this.page.getByText('BDD Sample Book', { exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
	await expect(this.page.getByText('Zeta Test Book', { exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});

	// Enter select mode.
	await this.page.getByRole('button', { name: 'Select', exact: true }).click();

	// Select both documents.
	const checkboxes = this.page.getByLabel('Select');
	await checkboxes.nth(0).click();
	await checkboxes.nth(1).click();

	// Click "Merge" in the batch toolbar.
	await this.page.getByRole('button', { name: 'Merge', exact: true }).click();

	// In the merge dialog, pick the first document as winner.
	await this.page
		.getByRole('radio')
		.first()
		.click();

	// Confirm the merge via the dialog's "Merge" button.
	// The dialog heading "Merge Documents" distinguishes the confirm button
	// from the toolbar "Merge" button.
	const dialog = this.page.locator('div').filter({ hasText: /^Merge Documents/ }).first();
	await expect(dialog).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
	await dialog.getByRole('button', { name: 'Merge', exact: true }).click();

	// Wait for the merge overlay to disappear (it has a fixed black backdrop).
	await expect(this.page.locator('h2', { hasText: 'Merge Documents' })).not.toBeVisible({
		timeout: MERGE_TIMEOUT,
	});
});

Then('only one document remains in the library', async function (this: BddWorld) {
	await this.page.goto(`${this.baseUrl}/`);
	// After the merge, navigate back and count document titles.
	// We wait for one to appear and then assert the other is gone.
	await expect(this.page.getByText('BDD Sample Book', { exact: true })).toBeVisible({
		timeout: SLOW_LOAD_TIMEOUT,
	});
	await expect(this.page.getByText('Zeta Test Book', { exact: true })).not.toBeVisible();
});
