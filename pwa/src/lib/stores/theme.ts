import { writable, derived, get } from 'svelte/store';
import { browser } from '$app/environment';

// ── Types ──────────────────────────────────────────────────────────────────

export type Mode = 'system' | 'light' | 'dark';
export type LightScheme = 'slate-light' | 'nord-light' | 'catppuccin-latte' | 'one-light' | 'custom-light';
export type DarkScheme =
	| 'slate-dark'
	| 'nord-dark'
	| 'catppuccin-frappe'
	| 'catppuccin-macchiato'
	| 'catppuccin-mocha'
	| 'one-dark'
	| 'custom-dark';
export type ColorScheme = LightScheme | DarkScheme;
// Backward-compat alias used by the epub viewer
export type Theme = ColorScheme;

export interface CustomColors {
	bg: string;       // page background
	surface: string;  // card/panel background
	accent: string;   // interactive highlight (buttons, focus rings, active states)
	text: string;     // primary body text color
}

export interface NamedTheme {
	id: string;
	name: string;
	colors: CustomColors;
}

// ── Constants ─────────────────────────────────────────────────────────────

const MODE_KEY          = 'read-flow-mode';
const LIGHT_KEY         = 'read-flow-light-scheme';
const DARK_KEY          = 'read-flow-dark-scheme';
const CUSTOM_COLORS_KEY = 'read-flow-custom-colors';
const NAMED_THEMES_KEY  = 'read-flow-named-themes';

const DEFAULT_CUSTOM_DARK: CustomColors  = { bg: '#1e293b', surface: '#334155', accent: '#3b82f6', text: '#e2e8f0' };
const DEFAULT_CUSTOM_LIGHT: CustomColors = { bg: '#f8fafc', surface: '#ffffff', accent: '#3b82f6', text: '#0f172a' };
// Default shown in the editor before the user picks light vs dark
const DEFAULT_CUSTOM_COLORS: CustomColors = DEFAULT_CUSTOM_DARK;

const DARK_SCHEMES = new Set<string>([
	'slate-dark', 'nord-dark',
	'catppuccin-frappe', 'catppuccin-macchiato', 'catppuccin-mocha',
	'one-dark', 'custom-dark',
]);

const CUSTOM_SCHEMES = new Set<string>([
	'nord-light', 'nord-dark',
	'catppuccin-latte', 'catppuccin-frappe', 'catppuccin-macchiato', 'catppuccin-mocha',
	'one-light', 'one-dark',
	'custom-light', 'custom-dark',
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
// @feature: theme.editor
export const customColors = writable<CustomColors>(DEFAULT_CUSTOM_COLORS);
export const namedThemes  = writable<NamedTheme[]>([]);

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

// ── Custom colour helpers ──────────────────────────────────────────────────

// NOTE: The interpolation weights below are mirrored in the no-FOUC inline
// script in app.html (_lerp). If you change coefficients here, update that
// script too.
export function hexToRgb(hex: string): [number, number, number] {
	const n = parseInt(hex.slice(1), 16);
	return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

export function rgbToHex(r: number, g: number, b: number): string {
	return '#' + [r, g, b]
		.map(v => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, '0'))
		.join('');
}

export function lerpColors(c1: string, c2: string, t: number): string {
	const [r1, g1, b1] = hexToRgb(c1);
	const [r2, g2, b2] = hexToRgb(c2);
	return rgbToHex(r1 + (r2 - r1) * t, g1 + (g2 - g1) * t, b1 + (b2 - b1) * t);
}

export function relativeLuminance(hex: string): number {
	const [r, g, b] = hexToRgb(hex).map(v => {
		const s = v / 255;
		return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** Returns true when bg luminance indicates a dark theme. */
export function isCustomDark(c: CustomColors): boolean {
	return relativeLuminance(c.bg) < 0.5;
}

// ── Custom CSS variable application ───────────────────────────────────────

function applyCustomVars({ bg, surface, accent, text }: CustomColors, dark: boolean): void {
	if (!browser) return;
	const set = (name: string, val: string) =>
		document.documentElement.style.setProperty(name, val);
	if (dark) {
		set('--color-slate-900', bg);
		set('--color-slate-800', surface);
		set('--color-slate-700', lerpColors(surface, text, 0.15));
		set('--color-slate-600', lerpColors(surface, text, 0.30));
		set('--color-slate-500', lerpColors(surface, text, 0.45));
		set('--color-slate-400', lerpColors(surface, text, 0.60));
		set('--color-slate-300', lerpColors(surface, text, 0.75));
		set('--color-slate-200', lerpColors(surface, text, 0.87));
		set('--color-slate-100', text);
		set('--color-slate-50',  lerpColors(text, '#ffffff', 0.25));
		set('--color-white',     lerpColors(text, '#ffffff', 0.12));
		set('--rf-bg',     bg);
		set('--rf-text',   text);
		set('--rf-accent', accent);
	} else {
		set('--color-slate-50',  bg);
		set('--color-white',     surface);
		set('--color-slate-100', lerpColors(bg, text, 0.08));
		set('--color-slate-200', lerpColors(bg, text, 0.18));
		set('--color-slate-300', lerpColors(bg, text, 0.30));
		set('--color-slate-400', lerpColors(bg, text, 0.45));
		set('--color-slate-500', lerpColors(bg, text, 0.58));
		set('--color-slate-600', lerpColors(bg, text, 0.70));
		set('--color-slate-700', lerpColors(bg, text, 0.82));
		set('--color-slate-800', lerpColors(bg, text, 0.90));
		set('--color-slate-900', text);
		set('--rf-bg',     surface);
		set('--rf-text',   text);
		set('--rf-accent', accent);
	}
}

function clearCustomVars(): void {
	if (!browser) return;
	[
		'--color-white', '--color-slate-50', '--color-slate-100', '--color-slate-200',
		'--color-slate-300', '--color-slate-400', '--color-slate-500', '--color-slate-600',
		'--color-slate-700', '--color-slate-800', '--color-slate-900',
		'--rf-bg', '--rf-text', '--rf-accent',
	].forEach(v => document.documentElement.style.removeProperty(v));
}

// ── DOM application ────────────────────────────────────────────────────────

function applyScheme(scheme: ColorScheme): void {
	if (!browser) return;
	if (scheme === 'custom-dark' || scheme === 'custom-light') {
		const colors = get(customColors);
		const dark = scheme === 'custom-dark';
		document.documentElement.classList.toggle('dark', dark);
		document.documentElement.setAttribute('data-scheme', scheme);
		applyCustomVars(colors, dark);
		return;
	}
	clearCustomVars();
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

/** Migrate old 3-field CustomColors (where accent was text) to 4-field format. */
function migrateCustomColors(raw: unknown): CustomColors {
	const c = raw as Record<string, string>;
	if (c.text) return c as unknown as CustomColors;
	// Old format: accent was the text color; pick a sensible default accent.
	const isDark = relativeLuminance(c.bg ?? '#1e293b') < 0.5;
	return {
		bg:      c.bg      ?? (isDark ? DEFAULT_CUSTOM_DARK.bg      : DEFAULT_CUSTOM_LIGHT.bg),
		surface: c.surface ?? (isDark ? DEFAULT_CUSTOM_DARK.surface  : DEFAULT_CUSTOM_LIGHT.surface),
		accent:  isDark ? DEFAULT_CUSTOM_DARK.accent : DEFAULT_CUSTOM_LIGHT.accent,
		text:    c.accent  ?? (isDark ? DEFAULT_CUSTOM_DARK.text     : DEFAULT_CUSTOM_LIGHT.text),
	};
}

// ── Named theme CRUD ───────────────────────────────────────────────────────

function persistNamedThemes(themes: NamedTheme[]): void {
	if (!browser) return;
	localStorage.setItem(NAMED_THEMES_KEY, JSON.stringify(themes));
	namedThemes.set(themes);
}

export function loadNamedThemes(): void {
	if (!browser) return;
	try {
		const raw = localStorage.getItem(NAMED_THEMES_KEY);
		if (raw) namedThemes.set(JSON.parse(raw) as NamedTheme[]);
	} catch { /* ignore */ }
}

export function saveNamedTheme(name: string, colors: CustomColors): void {
	const themes = get(namedThemes);
	const existing = themes.findIndex(t => t.name === name);
	const entry: NamedTheme = {
		id: existing >= 0 ? themes[existing].id : crypto.randomUUID(),
		name,
		colors,
	};
	persistNamedThemes(existing >= 0
		? themes.map((t, i) => i === existing ? entry : t)
		: [...themes, entry]);
}

export function deleteNamedTheme(id: string): void {
	persistNamedThemes(get(namedThemes).filter(t => t.id !== id));
}

export function exportThemes(): void {
	if (!browser) return;
	const json = JSON.stringify(get(namedThemes), null, 2);
	const url = URL.createObjectURL(new Blob([json], { type: 'application/json' }));
	const a = document.createElement('a');
	a.href = url;
	a.download = 'read-flow-themes.json';
	a.click();
	URL.revokeObjectURL(url);
}

export function importThemes(json: string): void {
	try {
		const imported = JSON.parse(json) as NamedTheme[];
		const current = get(namedThemes);
		const existingIds = new Set(current.map(t => t.id));
		persistNamedThemes([...current, ...imported.filter(t => !existingIds.has(t.id))]);
	} catch { /* ignore */ }
}

// ── Public API ─────────────────────────────────────────────────────────────

export function initTheme(): () => void {
	if (!browser) return () => {};

	migrate();
	loadNamedThemes();

	const savedMode  = (localStorage.getItem(MODE_KEY)  as Mode        | null) ?? 'system';
	const savedLight = (localStorage.getItem(LIGHT_KEY) as LightScheme | null) ?? 'slate-light';
	const savedDark  = (localStorage.getItem(DARK_KEY)  as DarkScheme  | null) ?? 'slate-dark';

	// Restore custom colors before subscribing so applyScheme can read them
	const storedCustom = localStorage.getItem(CUSTOM_COLORS_KEY);
	if (storedCustom) {
		try {
			customColors.set(migrateCustomColors(JSON.parse(storedCustom)));
		} catch { /* ignore */ }
	}

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

/**
 * Persist custom colors and activate the custom scheme.
 * Dark vs light is determined by the background luminance.
 */
export function setCustomColors(colors: CustomColors): void {
	if (!browser) return;
	localStorage.setItem(CUSTOM_COLORS_KEY, JSON.stringify(colors));
	customColors.set(colors);
	if (isCustomDark(colors)) {
		setDarkScheme('custom-dark');
	} else {
		setLightScheme('custom-light');
	}
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
