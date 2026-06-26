import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

// `Given a document has been added to the library` is in documents_list.steps.ts.

const SLOW_LOAD_TIMEOUT = 15_000;
const EPUB_LOAD_TIMEOUT = 30_000;

When('I open the document for reading', async function (this: BddWorld) {
	const fp = this.currentDocumentFingerprint ?? '';
	await this.page.goto(`${this.baseUrl}/read/epub/${fp}`);
});

Then('the EPUB content is displayed', async function (this: BddWorld) {
	// The epub.js viewer injects an <iframe> into viewerEl once loaded.
	// Wait for the "Loading EPUB…" spinner to disappear, then verify the iframe appears.
	await expect(this.page.getByText('Loading EPUB…')).not.toBeVisible({
		timeout: EPUB_LOAD_TIMEOUT,
	});
	// epub.js appends an iframe into the viewer div.
	const viewerIframe = this.page.locator('iframe').first();
	await expect(viewerIframe).toBeVisible({ timeout: SLOW_LOAD_TIMEOUT });
});
