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
	[backend, preview] = await Promise.all([spawnBackend(), spawnPreview()]);
	browser = await chromium.launch();
});

AfterAll(async function () {
	await browser?.close();
	await Promise.all([backend?.stop(), preview?.stop()]);
});

Before(async function (this: BddWorld) {
	const fixtures: SharedFixtures = { browser, backend, preview };
	this.fixtures = fixtures;
	this.context = await browser.newContext();
	this.page = await this.context.newPage();
});

After(async function (this: BddWorld) {
	await this.context?.close();
});
