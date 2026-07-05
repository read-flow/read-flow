import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

/**
 * Matches the credentials baked into `features/*.feature` (and the cucumber-rs
 * drivers' `rest_driver::USER`/`PASSWORD` constants) — one canonical Gherkin
 * spec, one canonical fixture identity, across every runner.
 *
 * Fixed PHC hash for the password "correct-horse", generated once via
 * `HashedPassword::with_rounds("correct-horse", Params::MIN_ROUNDS)` (1000
 * rounds — fast for tests). PBKDF2 verification only depends on the password
 * matching the embedded salt+hash, so a stable hash for a stable test
 * password is safe to commit and reuse across runs.
 */
const BDD_USER = 'alice';
const BDD_PASSWORD = 'correct-horse';
const BDD_PASSWORD_HASH =
	'$pbkdf2-sha256$i=1000,l=32$weEfAaEiLNy+ZsD/cKJU4Q$SwGeEOtgjBrNPzjcQW48C9VmCnEIQ+iBh020TMarMs0';

const REPO_ROOT = join(import.meta.dirname, '..', '..', '..');
const READ_FLOW_CLI = join(REPO_ROOT, 'target', 'debug', 'read-flow-cli');

export interface BackendHandle {
	baseUrl: string;
	user: string;
	password: string;
	stop(): Promise<void>;
}

export interface PreviewHandle {
	baseUrl: string;
	stop(): Promise<void>;
}

// eslint-disable-next-line no-control-regex
const ANSI_ESCAPES = /\x1b\[[0-9;]*[A-Za-z]/g;

function stripAnsi(text: string): string {
	return text.replace(ANSI_ESCAPES, '');
}

function waitForLine(proc: ChildProcess, pattern: RegExp, timeoutMs: number): Promise<RegExpMatchArray> {
	return new Promise((resolve, reject) => {
		const chunks: string[] = [];
		const timer = setTimeout(() => {
			cleanup();
			reject(new Error(`Timed out waiting for ${pattern} in output:\n${chunks.join('')}`));
		}, timeoutMs);

		function onData(data: Buffer): void {
			// Vite (and other tools) colorize their output; escape sequences can
			// land inside the URL (e.g. a bold port number), so match plain text.
			const text = stripAnsi(data.toString('utf-8'));
			chunks.push(text);
			const match = text.match(pattern);
			if (match) {
				cleanup();
				resolve(match);
			}
		}

		function cleanup(): void {
			clearTimeout(timer);
			proc.stdout?.off('data', onData);
			proc.stderr?.off('data', onData);
		}

		proc.stdout?.on('data', onData);
		proc.stderr?.on('data', onData);
	});
}

async function stopProcess(proc: ChildProcess): Promise<void> {
	if (proc.exitCode !== null || proc.killed) return;
	await new Promise<void>((resolve) => {
		proc.once('exit', () => resolve());
		proc.kill('SIGTERM');
		setTimeout(() => proc.kill('SIGKILL'), 5_000);
	});
}

async function waitForHttp(url: string, timeoutMs: number): Promise<void> {
	const deadline = Date.now() + timeoutMs;
	let lastError: unknown;
	while (Date.now() < deadline) {
		try {
			const res = await fetch(url);
			// Any HTTP response (even 401/404) proves the server is accepting connections.
			if (res.status > 0) return;
		} catch (err) {
			lastError = err;
		}
		await new Promise((resolve) => setTimeout(resolve, 200));
	}
	throw new Error(`Timed out waiting for ${url} to respond: ${String(lastError)}`);
}

/** Boots `read-flow-cli serve` against a fresh temp config + temp SQLite DB. */
export async function spawnBackend(): Promise<BackendHandle> {
	const dir = mkdtempSync(join(tmpdir(), 'read-flow-bdd-'));
	const configPath = join(dir, 'read-flow.toml');
	writeFileSync(
		configPath,
		[
			'[database]',
			`url = "${join(dir, 'test.db')}"`,
			'',
			'[server]',
			`download_folder = "${dir}"`,
			'',
			`[server.authorized_users.${BDD_USER}]`,
			`password = "${BDD_PASSWORD_HASH}"`,
			'roles = ["owner"]',
			'',
		].join('\n'),
	);

	const proc = spawn(READ_FLOW_CLI, ['--configuration-file', configPath, 'serve'], {
		env: { ...process.env, READ_FLOW_PORT: '0', READ_FLOW_ADDRESS: '127.0.0.1' },
		stdio: ['ignore', 'pipe', 'pipe'],
	});

	const match = await waitForLine(proc, /Server listening on (http:\/\/127\.0\.0\.1:\d+)/, 30_000);
	const baseUrl = match[1];
	await waitForHttp(`${baseUrl}/status`, 10_000);

	return {
		baseUrl,
		user: BDD_USER,
		password: BDD_PASSWORD,
		async stop() {
			await stopProcess(proc);
			rmSync(dir, { recursive: true, force: true });
		},
	};
}

/** Serves the production PWA build (`vite preview`) on an OS-assigned port. */
export async function spawnPreview(): Promise<PreviewHandle> {
	const proc = spawn(join(REPO_ROOT, 'pwa', 'node_modules', '.bin', 'vite'), ['preview', '--port', '0'], {
		cwd: join(REPO_ROOT, 'pwa'),
		stdio: ['ignore', 'pipe', 'pipe'],
	});

	const match = await waitForLine(proc, /Local:\s+(http:\/\/localhost:\d+)/, 30_000);
	const baseUrl = match[1];
	await waitForHttp(baseUrl, 10_000);

	return {
		baseUrl,
		async stop() {
			await stopProcess(proc);
		},
	};
}
