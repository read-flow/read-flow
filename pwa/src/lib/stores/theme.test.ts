import { describe, it, expect } from 'vitest';
import {
	isDarkScheme, cycleMode, modeIcon, modeLabel,
	hexToRgb, rgbToHex, lerpColors, relativeLuminance, isCustomDark,
	type Mode, type CustomColors,
} from './theme';

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

// ── Color math ────────────────────────────────────────────────────────────────

describe('hexToRgb', () => {
	it('parses black', () => {
		expect(hexToRgb('#000000')).toEqual([0, 0, 0]);
	});

	it('parses white', () => {
		expect(hexToRgb('#ffffff')).toEqual([255, 255, 255]);
	});

	it('parses a mid-range colour', () => {
		expect(hexToRgb('#1e293b')).toEqual([0x1e, 0x29, 0x3b]);
	});
});

describe('rgbToHex', () => {
	it('encodes black', () => {
		expect(rgbToHex(0, 0, 0)).toBe('#000000');
	});

	it('encodes white', () => {
		expect(rgbToHex(255, 255, 255)).toBe('#ffffff');
	});

	it('clamps values outside 0–255', () => {
		expect(rgbToHex(-10, 300, 128)).toBe('#00ff80');
	});
});

describe('lerpColors', () => {
	it('returns c1 at t=0', () => {
		expect(lerpColors('#000000', '#ffffff', 0)).toBe('#000000');
	});

	it('returns c2 at t=1', () => {
		expect(lerpColors('#000000', '#ffffff', 1)).toBe('#ffffff');
	});

	it('returns the midpoint at t=0.5', () => {
		// Math.round(127.5) = 128 in JS, so the midpoint rounds to #808080
		const mid = lerpColors('#000000', '#ffffff', 0.5);
		const [r, g, b] = hexToRgb(mid);
		// Each channel should be within 1 of the exact midpoint (127.5)
		expect(r).toBeGreaterThanOrEqual(127);
		expect(r).toBeLessThanOrEqual(128);
		expect(g).toBe(r);
		expect(b).toBe(r);
	});

	it('is symmetric: lerp(a, b, t) and lerp(b, a, 1-t) produce the same result', () => {
		const ab = lerpColors('#1e293b', '#e2e8f0', 0.3);
		const ba = lerpColors('#e2e8f0', '#1e293b', 0.7);
		expect(ab).toBe(ba);
	});
});

describe('relativeLuminance', () => {
	it('returns 0 for black', () => {
		expect(relativeLuminance('#000000')).toBeCloseTo(0, 5);
	});

	it('returns 1 for white', () => {
		expect(relativeLuminance('#ffffff')).toBeCloseTo(1, 5);
	});

	it('returns a value in [0, 1] for any colour', () => {
		const lum = relativeLuminance('#1e293b');
		expect(lum).toBeGreaterThanOrEqual(0);
		expect(lum).toBeLessThanOrEqual(1);
	});
});

describe('isCustomDark', () => {
	it('returns true for a dark background', () => {
		const dark: CustomColors = { bg: '#1e293b', surface: '#334155', accent: '#3b82f6', text: '#e2e8f0' };
		expect(isCustomDark(dark)).toBe(true);
	});

	it('returns false for a light background', () => {
		const light: CustomColors = { bg: '#f8fafc', surface: '#ffffff', accent: '#3b82f6', text: '#0f172a' };
		expect(isCustomDark(light)).toBe(false);
	});

	it('returns false for a pure white background', () => {
		expect(isCustomDark({ bg: '#ffffff', surface: '#f0f0f0', accent: '#3b82f6', text: '#333333' })).toBe(false);
	});

	it('returns true for a pure black background', () => {
		expect(isCustomDark({ bg: '#000000', surface: '#111111', accent: '#3b82f6', text: '#eeeeee' })).toBe(true);
	});
});
