import { setWorldConstructor, World as CucumberWorld } from '@cucumber/cucumber';
import type { Browser, BrowserContext, Page } from 'playwright';
import type { BackendHandle, PreviewHandle } from './server';

export interface SharedFixtures {
	browser: Browser;
	backend: BackendHandle;
	preview: PreviewHandle;
}

/** Per-scenario state. The browser/backend/preview are shared (booted once in BeforeAll). */
export class BddWorld extends CucumberWorld {
	fixtures!: SharedFixtures;
	context!: BrowserContext;
	page!: Page;
	/** GUID of the most recently seeded document — set by seed steps. */
	currentDocumentGuid?: string;
	/** Fingerprint of the most recently seeded document — set by seed steps. */
	currentDocumentFingerprint?: string;
	/** Document-record GUID of the most recently seeded document — set by seed steps. */
	currentDocumentApiGuid?: string;

	get baseUrl(): string {
		return this.fixtures.preview.baseUrl;
	}
}

setWorldConstructor(BddWorld);
