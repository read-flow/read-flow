import { describe, it, expect } from 'vitest';
import { isDarkScheme, cycleMode, modeIcon, modeLabel, type Mode } from './theme';

describe('isDarkScheme', () => {
	it('returns true for all dark schemes', () => {
		const darkSchemes = [
			'slate-dark',
			'nord-dark',
			'catppuccin-frappe',
			'catppuccin-macchiato',
			'catppuccin-mocha',
			'one-dark',
		];
		darkSchemes.forEach((s) => expect(isDarkScheme(s), s).toBe(true));
	});

	it('returns false for all light schemes', () => {
		const lightSchemes = ['slate-light', 'nord-light', 'catppuccin-latte', 'one-light'];
		lightSchemes.forEach((s) => expect(isDarkScheme(s), s).toBe(false));
	});

	it('returns false for an unknown scheme', () => {
		expect(isDarkScheme('unknown-scheme')).toBe(false);
	});
});

describe('cycleMode', () => {
	it('cycles system → light', () => {
		// cycleMode calls setMode internally; since browser=false in tests, setMode is a no-op.
		// We verify the function doesn't throw and completes without error.
		expect(() => cycleMode('system')).not.toThrow();
	});

	it('cycles through all three modes without throwing', () => {
		const modes: Mode[] = ['system', 'light', 'dark'];
		modes.forEach((m) => expect(() => cycleMode(m)).not.toThrow());
	});
});

describe('modeIcon', () => {
	it('returns "monitor" for system mode', () => {
		expect(modeIcon('system')).toBe('monitor');
	});

	it('returns "sun" for light mode', () => {
		expect(modeIcon('light')).toBe('sun');
	});

	it('returns "moon" for dark mode', () => {
		expect(modeIcon('dark')).toBe('moon');
	});
});

describe('modeLabel', () => {
	it('returns "System" for system mode', () => {
		expect(modeLabel('system')).toBe('System');
	});

	it('returns "Light" for light mode', () => {
		expect(modeLabel('light')).toBe('Light');
	});

	it('returns "Dark" for dark mode', () => {
		expect(modeLabel('dark')).toBe('Dark');
	});
});
