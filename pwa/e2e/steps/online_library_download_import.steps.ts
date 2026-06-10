import { fileURLToPath } from 'url';
import path from 'path';
import { readFileSync } from 'fs';
import { createServer } from 'http';
import { Then, When } from '@cucumber/cucumber';
import { expect } from '@playwright/test';
import type { BddWorld } from '../support/world';

const EPUB_FIXTURE_PATH = path.join(
	path.dirname(fileURLToPath(import.meta.url)),
	'..', '..', '..', 'features', 'fixtures', 'sample.epub',
);

function basicAuthHeader(user: string, password: string): string {
	return `Basic ${Buffer.from(`${user}:${password}`).toString('base64')}`;
}

/** Serves sample.epub once via a local HTTP server, returns the URL. */
async function serveEpubOnce(): Promise<string> {
	const epubBytes = readFileSync(EPUB_FIXTURE_PATH);
	return new Promise((resolve) => {
		const server = createServer((_req, res) => {
			res.writeHead(200, {
				'Content-Type': 'application/epub+zip',
				'Content-Length': epubBytes.length,
				Connection: 'close',
			});
			res.end(epubBytes);
			server.close();
		});
		server.listen(0, '127.0.0.1', () => {
			const addr = server.address() as { port: number };
			resolve(`http://127.0.0.1:${addr.port}/sample.epub`);
		});
	});
}

// `Given a read-flow server is running…` is in common.steps.ts.

When('I import a book from the online library', async function (this: BddWorld) {
	const url = await serveEpubOnce();
	const { baseUrl, user, password } = this.fixtures.backend;
	const auth = basicAuthHeader(user, password);
	const res = await fetch(`${baseUrl}/online-library/import`, {
		method: 'POST',
		headers: { Authorization: auth, 'Content-Type': 'application/json' },
		body: JSON.stringify({
			title: 'BDD Sample Book',
			format: { mime_type: 'application/epub+zip', href: url, label: 'EPUB' },
		}),
	});
	this.currentDocumentFingerprint = res.ok ? 'ok' : '';
});

Then('the book was imported successfully', async function (this: BddWorld) {
	expect(this.currentDocumentFingerprint).toBe('ok');
});
