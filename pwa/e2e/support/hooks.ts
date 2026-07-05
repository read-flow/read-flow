import { After, AfterAll, Before, BeforeAll, setDefaultTimeout } from '@cucumber/cucumber';
import { chromium } from 'playwright';
import { spawnBackend, spawnPreview, type BackendHandle, type PreviewHandle } from './server';
import type { BddWorld, SharedFixtures } from './world';
import type { Browser } from 'playwright';

setDefaultTimeout(60_000);

let browser: Browser;
let backend: BackendHandle;
let preview: PreviewHandle;

BeforeAll(async function () {
	preview = await spawnPreview();
	browser = await chromium.launch();
});

AfterAll(async function () {
	await browser?.close();
	await preview?.stop();
});

Before(async function (this: BddWorld) {
	// Fresh backend per scenario: scenarios mutate server state (rename titles,
	// attach formats, delete files), so sharing one DB leaks state between them.
	// The browser context is fresh too, so the PWA's IndexedDB starts empty and
	// each scenario registers its source(s) itself.
	backend = await spawnBackend();
	const fixtures: SharedFixtures = { browser, backend, preview };
	this.fixtures = fixtures;
	this.context = await browser.newContext();
	this.page = await this.context.newPage();
});

After(async function (this: BddWorld) {
	await this.context?.close();
	await backend?.stop();
});
