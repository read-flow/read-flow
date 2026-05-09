import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type ColorScheme =
	| 'system'
	| 'slate-light'
	| 'slate-dark'
	| 'nord-light'
	| 'nord-dark'
	| 'catppuccin-latte'
	| 'catppuccin-frappe'
	| 'catppuccin-macchiato'
	| 'catppuccin-mocha'
	| 'one-light'
	| 'one-dark';

// Keep Theme as an alias so existing imports don't break
export type Theme = ColorScheme;

const STORAGE_KEY = 'read-flow-scheme';

const DARK_SCHEMES = new Set<ColorScheme>([
	'slate-dark',
	'nord-dark',
	'catppuccin-frappe',
	'catppuccin-macchiato',
	'catppuccin-mocha',
	'one-dark',
]);

// Schemes that need a data-scheme attribute (non-default slate palette)
const CUSTOM_SCHEMES = new Set<ColorScheme>([
	'nord-light',
	'nord-dark',
	'catppuccin-latte',
	'catppuccin-frappe',
	'catppuccin-macchiato',
	'catppuccin-mocha',
	'one-light',
	'one-dark',
]);

export function isDarkScheme(scheme: ColorScheme): boolean {
	if (scheme === 'system') {
		return browser ? window.matchMedia('(prefers-color-scheme: dark)').matches : false;
	}
	return DARK_SCHEMES.has(scheme);
}

function getSaved(): ColorScheme {
	if (!browser) return 'system';

	// Migrate old 'read-flow-theme' key (light → slate-light, dark → slate-dark)
	const old = localStorage.getItem('read-flow-theme');
	if (old && !localStorage.getItem(STORAGE_KEY)) {
		const migrated: ColorScheme =
			old === 'light' ? 'slate-light' : old === 'dark' ? 'slate-dark' : 'system';
		if (migrated !== 'system') localStorage.setItem(STORAGE_KEY, migrated);
		localStorage.removeItem('read-flow-theme');
		return migrated;
	}

	return (localStorage.getItem(STORAGE_KEY) as ColorScheme | null) ?? 'system';
}

function apply(scheme: ColorScheme): void {
	if (!browser) return;
	document.documentElement.classList.toggle('dark', isDarkScheme(scheme));
	if (CUSTOM_SCHEMES.has(scheme)) {
		document.documentElement.setAttribute('data-scheme', scheme);
	} else {
		document.documentElement.removeAttribute('data-scheme');
	}
}

export const theme = writable<ColorScheme>('system');

export function initTheme(): () => void {
	const saved = getSaved();
	theme.set(saved);
	apply(saved);

	const mq = window.matchMedia('(prefers-color-scheme: dark)');
	function onOsChange() {
		theme.update((t) => {
			apply(t);
			return t;
		});
	}
	mq.addEventListener('change', onOsChange);
	return () => mq.removeEventListener('change', onOsChange);
}

export function setTheme(scheme: ColorScheme): void {
	if (!browser) return;
	if (scheme === 'system') {
		localStorage.removeItem(STORAGE_KEY);
	} else {
		localStorage.setItem(STORAGE_KEY, scheme);
	}
	apply(scheme);
	theme.set(scheme);
}

// Quick toggle used by the sidebar/mobile button: cycles system → slate-light → slate-dark
export function cycleTheme(current: ColorScheme): void {
	if (current === 'system') setTheme('slate-light');
	else if (current === 'slate-light') setTheme('slate-dark');
	else setTheme('system');
}

// Helper for the layout's icon / label
export function schemeIcon(scheme: ColorScheme): 'monitor' | 'sun' | 'moon' {
	if (scheme === 'system') return 'monitor';
	return isDarkScheme(scheme) ? 'moon' : 'sun';
}

export function schemeShortLabel(scheme: ColorScheme): string {
	const labels: Record<ColorScheme, string> = {
		system: 'System',
		'slate-light': 'Light',
		'slate-dark': 'Dark',
		'nord-light': 'Nord',
		'nord-dark': 'Nord',
		'catppuccin-latte': 'Latte',
		'catppuccin-frappe': 'Frappé',
		'catppuccin-macchiato': 'Macc.',
		'catppuccin-mocha': 'Mocha',
		'one-light': 'One',
		'one-dark': 'One',
	};
	return labels[scheme];
}
