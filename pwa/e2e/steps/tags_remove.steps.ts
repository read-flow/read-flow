import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_AUTH_TIMEOUT = 15_000;

async function goToDocumentDetail(world: BddWorld): Promise<void> {
	const fingerprint = world.currentDocumentFingerprint;
	if (!fingerprint) throw new Error('no current document fingerprint — seed step must run first');
	await world.page.goto(`${world.baseUrl}/documents/${fingerprint}`);
	await expect(world.page.getByPlaceholder('Add a tag…')).toBeVisible({
		timeout: SLOW_AUTH_TIMEOUT,
	});
}

When(
	'I remove the tag {string} from the document',
	async function (this: BddWorld, tag: string) {
		await goToDocumentDetail(this);
		await this.page.getByRole('button', { name: `Remove tag ${tag}` }).click();
		// Wait for the tag to disappear before returning.
		await expect(this.page.getByText(tag, { exact: true })).toBeHidden({
			timeout: SLOW_AUTH_TIMEOUT,
		});
	},
);

Then(
	"{string} no longer appears in the document's tag list",
	async function (this: BddWorld, tag: string) {
		await expect(this.page.getByText(tag, { exact: true })).toBeHidden();
	},
);
