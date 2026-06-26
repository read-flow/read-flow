import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

// `Given a read-flow server is running…` is in common.steps.ts.
// `And a document with a cover image has been added to the library` is in
//   documents_cover_display.steps.ts.
// `Then a cover image is returned` is in documents_cover_display.steps.ts.

function basicAuthHeader(user: string, password: string): string {
	return `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`;
}

When(
	"I set the document's cover to its file's cover image",
	async function (this: BddWorld) {
		const { baseUrl, user, password } = this.fixtures.backend;
		const auth = basicAuthHeader(user, password);
		const docGuid = this.currentDocumentApiGuid;
		const fingerprint = this.currentDocumentFingerprint;
		expect(docGuid, 'document guid must be set').toBeTruthy();
		expect(fingerprint, 'fingerprint must be set').toBeTruthy();
		const res = await fetch(`${baseUrl}/documents/${docGuid}/metadata`, {
			method: 'PUT',
			headers: { Authorization: auth, 'Content-Type': 'application/json' },
			body: JSON.stringify({ selected_cover_fingerprint: fingerprint }),
		});
		expect(res.ok, `PUT /documents/${docGuid}/metadata failed: ${res.status}`).toBe(true);
	},
);
