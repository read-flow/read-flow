import { When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

// "Then … appears in the document's tag list" is defined in tags_add.steps.ts.

const SLOW_LOAD_TIMEOUT = 15_000;

When(
	'I batch-add tag {string} to the selected documents',
	async function (this: BddWorld, tag: string) {
		await this.page.goto(`${this.baseUrl}/library`);

		// Wait for documents to load (source must already be registered by the seed step).
		await expect(this.page.getByText('BDD Sample Book', { exact: true })).toBeVisible({
			timeout: SLOW_LOAD_TIMEOUT,
		});

		// Enter select mode.
		await this.page.getByRole('button', { name: 'Select', exact: true }).click();

		// Select the seeded document via its checkbox.
		const fingerprint = this.currentDocumentFingerprint;
		if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
		await this.page.getByLabel('Select').first().click();

		// Type the tag into the batch-tag input and click "Add tag".
		await this.page.getByPlaceholder('tag…').fill(tag);
		await this.page.getByRole('button', { name: 'Add tag', exact: true }).click();

		// Wait for the add to complete — toolbar disappears once deselected, so
		// we just verify the tag appears on the document card.
		await expect(this.page.getByText(tag, { exact: true }).first()).toBeVisible({
			timeout: SLOW_LOAD_TIMEOUT,
		});
	},
);
