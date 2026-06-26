import { Given, Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

Given('a read-flow server is running', async function (this: BddWorld) {
	// Backend is booted once in BeforeAll (support/hooks.ts) — this step just
	// documents the precondition for the reader of the .feature file.
	expect(this.fixtures.backend.baseUrl).toMatch(/^http:\/\/127\.0\.0\.1:\d+$/);
});

When('I open the app', async function (this: BddWorld) {
	await this.page.goto(this.baseUrl);
});

Then('I see the library heading', async function (this: BddWorld) {
	await expect(this.page.getByRole('heading', { name: 'Library' })).toBeVisible();
});
