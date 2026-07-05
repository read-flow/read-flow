import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { Then } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const SLOW_LOAD_TIMEOUT = 15_000;

// `Given a read-flow server is running…` is in common.steps.ts.
// `And a document has been added to the library` is in documents_list.steps.ts.

// The PWA uses virtual (windowed) scrolling — no traditional page navigation.
// The initial view renders the first N items, so a newly seeded document must
// be visible in the viewport without scrolling.
Then(
	'{string} appears on the first page of the document list',
	async function (this: BddWorld, title: string) {
		await this.page.goto(`${this.baseUrl}/library`);
		await expect(this.page.getByText(title, { exact: true })).toBeVisible({
			timeout: SLOW_LOAD_TIMEOUT,
		});
	},
);
