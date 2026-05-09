import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type Theme = 'system' | 'light' | 'dark';

const STORAGE_KEY = 'read-flow-theme';
const CYCLE_ORDER: Theme[] = ['system', 'light', 'dark'];

function getSaved(): Theme {
	if (!browser) return 'system';
	return (localStorage.getItem(STORAGE_KEY) as Theme | null) ?? 'system';
}

function apply(t: Theme): void {
	if (!browser) return;
	const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
	const dark = t === 'dark' || (t === 'system' && prefersDark);
	document.documentElement.classList.toggle('dark', dark);
}

export const theme = writable<Theme>('system');

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

export function setTheme(t: Theme): void {
	if (!browser) return;
	if (t === 'system') {
		localStorage.removeItem(STORAGE_KEY);
	} else {
		localStorage.setItem(STORAGE_KEY, t);
	}
	apply(t);
	theme.set(t);
}

export function cycleTheme(current: Theme): void {
	setTheme(CYCLE_ORDER[(CYCLE_ORDER.indexOf(current) + 1) % CYCLE_ORDER.length]);
}
