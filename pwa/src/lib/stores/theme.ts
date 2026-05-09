import { writable, derived } from 'svelte/store';
import { browser } from '$app/environment';

// ── Types ──────────────────────────────────────────────────────────────────

export type Mode = 'system' | 'light' | 'dark';
export type LightScheme = 'slate-light' | 'nord-light' | 'catppuccin-latte' | 'one-light';
export type DarkScheme =
	| 'slate-dark'
	| 'nord-dark'
	| 'catppuccin-frappe'
	| 'catppuccin-macchiato'
	| 'catppuccin-mocha'
	| 'one-dark';
export type ColorScheme = LightScheme | DarkScheme;
// Backward-compat alias used by the epub viewer
export type Theme = ColorScheme;

// ── Constants ─────────────────────────────────────────────────────────────

const MODE_KEY  = 'read-flow-mode';
const LIGHT_KEY = 'read-flow-light-scheme';
const DARK_KEY  = 'read-flow-dark-scheme';

const DARK_SCHEMES = new Set<string>([
	'slate-dark', 'nord-dark',
	'catppuccin-frappe', 'catppuccin-macchiato', 'catppuccin-mocha',
	'one-dark',
]);

const CUSTOM_SCHEMES = new Set<string>([
	'nord-light', 'nord-dark',
	'catppuccin-latte', 'catppuccin-frappe', 'catppuccin-macchiato', 'catppuccin-mocha',
	'one-light', 'one-dark',
]);

export function isDarkScheme(scheme: string): boolean {
	return DARK_SCHEMES.has(scheme);
}

// ── Stores ─────────────────────────────────────────────────────────────────

// OS dark-mode preference, updated live by a MediaQueryList listener
const _prefersDark = writable(false);

export const mode        = writable<Mode>('system');
export const lightScheme = writable<LightScheme>('slate-light');
export const darkScheme  = writable<DarkScheme>('slate-dark');

/**
 * The colour scheme currently applied to the page.
 * - 'light' mode → always the selected light scheme
 * - 'dark'  mode → always the selected dark scheme
 * - 'system'     → follows OS preference, switching between the two selections
 */
export const theme = derived(
	[mode, lightScheme, darkScheme, _prefersDark],
	([$mode, $light, $dark, $pref]): ColorScheme => {
		if ($mode === 'light') return $light;
		if ($mode === 'dark')  return $dark;
		return $pref ? $dark : $light;
	},
);

// ── DOM application ────────────────────────────────────────────────────────

function applyScheme(scheme: ColorScheme): void {
	if (!browser) return;
	document.documentElement.classList.toggle('dark', DARK_SCHEMES.has(scheme));
	if (CUSTOM_SCHEMES.has(scheme)) {
		document.documentElement.setAttribute('data-scheme', scheme);
	} else {
		document.documentElement.removeAttribute('data-scheme');
	}
}

// ── Migration ──────────────────────────────────────────────────────────────

function migrate(): void {
	// From the previous intermediate key 'read-flow-scheme'
	const prev = localStorage.getItem('read-flow-scheme');
	if (prev && !localStorage.getItem(MODE_KEY)) {
		if (isDarkScheme(prev)) {
			localStorage.setItem(MODE_KEY,  'dark');
			localStorage.setItem(DARK_KEY,  prev);
		} else if (prev !== 'system') {
			localStorage.setItem(MODE_KEY,  'light');
			localStorage.setItem(LIGHT_KEY, prev);
		}
		localStorage.removeItem('read-flow-scheme');
	}
	// From the original key 'read-flow-theme'
	const oldest = localStorage.getItem('read-flow-theme');
	if (oldest && !localStorage.getItem(MODE_KEY)) {
		if (oldest === 'dark')  localStorage.setItem(MODE_KEY, 'dark');
		if (oldest === 'light') localStorage.setItem(MODE_KEY, 'light');
		localStorage.removeItem('read-flow-theme');
	}
}

// ── Public API ─────────────────────────────────────────────────────────────

export function initTheme(): () => void {
	if (!browser) return () => {};

	migrate();

	const savedMode  = (localStorage.getItem(MODE_KEY)  as Mode        | null) ?? 'system';
	const savedLight = (localStorage.getItem(LIGHT_KEY) as LightScheme | null) ?? 'slate-light';
	const savedDark  = (localStorage.getItem(DARK_KEY)  as DarkScheme  | null) ?? 'slate-dark';

	_prefersDark.set(window.matchMedia('(prefers-color-scheme: dark)').matches);
	mode.set(savedMode);
	lightScheme.set(savedLight);
	darkScheme.set(savedDark);

	// Keep the DOM in sync whenever any of the three inputs change
	const unsubTheme = theme.subscribe(applyScheme);

	// Track live OS preference changes
	const mq = window.matchMedia('(prefers-color-scheme: dark)');
	const onOsChange = (e: MediaQueryListEvent) => _prefersDark.set(e.matches);
	mq.addEventListener('change', onOsChange);

	return () => {
		unsubTheme();
		mq.removeEventListener('change', onOsChange);
	};
}

export function setMode(m: Mode): void {
	if (!browser) return;
	if (m === 'system') localStorage.removeItem(MODE_KEY);
	else localStorage.setItem(MODE_KEY, m);
	mode.set(m);
}

export function setLightScheme(s: LightScheme): void {
	if (!browser) return;
	localStorage.setItem(LIGHT_KEY, s);
	lightScheme.set(s);
}

export function setDarkScheme(s: DarkScheme): void {
	if (!browser) return;
	localStorage.setItem(DARK_KEY, s);
	darkScheme.set(s);
}

/** Quick toggle used by the sidebar / mobile button. */
export function cycleMode(current: Mode): void {
	const ORDER: Mode[] = ['system', 'light', 'dark'];
	setMode(ORDER[(ORDER.indexOf(current) + 1) % ORDER.length]);
}

export function modeIcon(m: Mode): 'monitor' | 'sun' | 'moon' {
	return m === 'system' ? 'monitor' : m === 'light' ? 'sun' : 'moon';
}

export function modeLabel(m: Mode): string {
	return m === 'system' ? 'System' : m === 'light' ? 'Light' : 'Dark';
}
