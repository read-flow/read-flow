<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import ePub from 'epubjs';
	import type { Rendition, Location } from 'epubjs';
	import Icon from '$lib/components/Icon.svelte';
	import { allDocuments, refreshDocuments } from '$lib/stores/documents';
	import { downloadFileFromSources, fetchReadingState, saveReadingState } from '$lib/api/aggregator';
	import { loadSources } from '$lib/stores/sources';
	import { theme } from '$lib/stores/theme';
	import { db } from '$lib/db';
	import { get } from 'svelte/store';
	import type { AggregatedFile } from '$lib/api/aggregator';

	const PREF_FONT_SIZE_KEY = 'epub-font-size';
	const RESIZE_DEBOUNCE_MS = 100;
	const TOOLBAR_HIDE_DELAY_MS = 3000;
	const PROGRESS_SAVE_DEBOUNCE_MS = 2000;
	const SWIPE_THRESHOLD_PX = 50;
	const FONT_SIZE_STEP = 10;
	const FONT_SIZE_MIN = 75;
	const FONT_SIZE_MAX = 200;

	// ── Route param ────────────────────────────────────────────────────────────
	const fingerprint = $derived($page.params.fingerprint ?? '');

	// ── Document and book state ────────────────────────────────────────────────
	let doc = $state<AggregatedFile | null>(null);
	// Not reactive — mutations don't need to trigger re-renders
	let book: ReturnType<typeof ePub> | null = null;
	let rendition: Rendition | null = null;
	let renditionReady = $state(false); // drives disabled state on nav buttons

	let spineIndex = $state(0);   // 0-based position from 'relocated' event
	let totalSpine = $state(0);   // book.spine.length after ready
	let percentage = $state(0);   // 0–100, from location.start.percentage × 100
	let fontSize = $state(100);   // CSS font-size percentage applied to the rendition
	let isLoading = $state(true);
	let loadError = $state<string | null>(null);

	// ── Toolbar visibility (mobile auto-hide) ──────────────────────────────────
	let toolbarVisible = $state(true);
	let toolbarTimer: ReturnType<typeof setTimeout> | null = null;

	// ── DOM ref ────────────────────────────────────────────────────────────────
	let viewerEl: HTMLDivElement | undefined = $state();

	// ── Timers / observers / subscriptions ───────────────────────────────────
	let resizeTimer: ReturnType<typeof setTimeout> | null = null;
	let progressTimer: ReturnType<typeof setTimeout> | null = null;
	let resizeObserver: ResizeObserver | null = null;
	let themeUnsubscribe: (() => void) | null = null;

	// ── Touch tracking ────────────────────────────────────────────────────────
	let touchStartX = 0;
	let touchStartY = 0;

	// ── Helpers ───────────────────────────────────────────────────────────────
	function basename(path: string): string {
		return path.split('/').pop() ?? path;
	}

	// ── Navigation ────────────────────────────────────────────────────────────
	async function navigate(direction: 'next' | 'prev'): Promise<void> {
		if (!rendition) return;
		if (direction === 'next') await rendition.next();
		else await rendition.prev();
		resetToolbarTimer();
	}

	// ── Reading theme ─────────────────────────────────────────────────────────
	// Read the active scheme's colours from the computed CSS variables so the
	// epub.js iframe always matches whichever colour scheme is currently active.
	function applyEpubTheme(dark: boolean): void {
		if (!rendition) return;
		const cs = getComputedStyle(document.documentElement);
		const bg   = cs.getPropertyValue(dark ? '--color-slate-900' : '--color-white').trim()
		             || (dark ? '#1e293b' : '#ffffff');
		const text = cs.getPropertyValue(dark ? '--color-slate-100' : '--color-slate-900').trim()
		             || (dark ? '#f1f5f9' : '#0f172a');
		const name = dark ? 'dark' : 'light';
		rendition.themes.register(name, {
			html: { background: bg, color: text },
			body: { background: `${bg} !important`, color: `${text} !important` },
			...(dark ? { a: { color: '#93c5fd !important' } } : {}),
		});
		rendition.themes.select(name);
	}

	// ── Font size ─────────────────────────────────────────────────────────────
	async function adjustFontSize(delta: number): Promise<void> {
		fontSize = Math.max(FONT_SIZE_MIN, Math.min(FONT_SIZE_MAX, fontSize + delta));
		rendition?.themes.fontSize(`${fontSize}%`);
		await db.preferences.put({ key: PREF_FONT_SIZE_KEY, value: String(fontSize) });
	}

	// ── Reading progress ──────────────────────────────────────────────────────
	function scheduleProgressSave(cfi: string, pct: number): void {
		if (progressTimer) clearTimeout(progressTimer);
		progressTimer = setTimeout(async () => {
			const now = new Date().toISOString();
			const fp = fingerprint;
			try {
				await saveReadingState({
					fingerprint: fp,
					status: 0,
					position: JSON.stringify({ cfi }),
					percentage: pct,
					last_updated: now,
					status_updated_at: '1970-01-01T00:00:00Z',
				});
			} catch (e) {
				console.warn('Failed to save EPUB progress:', e);
			}
		}, PROGRESS_SAVE_DEBOUNCE_MS);
	}

	// ── Toolbar ───────────────────────────────────────────────────────────────
	function resetToolbarTimer(): void {
		toolbarVisible = true;
		if (toolbarTimer) clearTimeout(toolbarTimer);
		toolbarTimer = setTimeout(() => {
			toolbarVisible = false;
		}, TOOLBAR_HIDE_DELAY_MS);
	}

	function handleViewerTap(): void {
		resetToolbarTimer();
	}

	// ── Keyboard ──────────────────────────────────────────────────────────────
	function handleKeydown(e: KeyboardEvent): void {
		if (e.target instanceof HTMLInputElement) return;
		switch (e.key) {
			case 'ArrowRight':
			case 'ArrowDown':
			case ' ':
				e.preventDefault();
				void navigate('next');
				break;
			case 'ArrowLeft':
			case 'ArrowUp':
				e.preventDefault();
				void navigate('prev');
				break;
		}
	}

	// ── Touch / swipe / side-tap zones ───────────────────────────────────────
	function handleTouchStart(e: TouchEvent): void {
		touchStartX = e.touches[0].clientX;
		touchStartY = e.touches[0].clientY;
	}

	function handleTouchEnd(e: TouchEvent): void {
		const touch = e.changedTouches[0];
		const dx = touch.clientX - touchStartX;
		const dy = touch.clientY - touchStartY;

		if (Math.abs(dx) >= SWIPE_THRESHOLD_PX) {
			void navigate(dx < 0 ? 'next' : 'prev');
		} else if (Math.abs(dx) < 10 && Math.abs(dy) < 10) {
			// True tap — check left/right zones
			const x = touch.clientX;
			const w = window.innerWidth;
			if (x < w * 0.2) {
				void navigate('prev');
			} else if (x > w * 0.8) {
				void navigate('next');
			} else {
				resetToolbarTimer();
			}
		} else {
			resetToolbarTimer();
		}
	}

	// ── Lifecycle ─────────────────────────────────────────────────────────────
	onMount(async () => {
		// Restore saved font size preference
		const savedSize = await db.preferences.get(PREF_FONT_SIZE_KEY);
		if (savedSize) {
			const parsed = parseInt(savedSize.value, 10);
			if (!isNaN(parsed)) fontSize = Math.max(FONT_SIZE_MIN, Math.min(FONT_SIZE_MAX, parsed));
		}

		await loadSources();

		// Look up the document
		let docs = get(allDocuments);
		if (docs.length === 0) await refreshDocuments();
		docs = get(allDocuments);
		doc =
			docs.find((d) => d.fingerprint === fingerprint) ??
			docs.flatMap((d) => d.otherFormats).find((d) => d.fingerprint === fingerprint) ??
			null;

		if (!doc) {
			loadError =
				'Document not found. Make sure at least one source is configured and the library has loaded.';
			isLoading = false;
			return;
		}

		if (!viewerEl) {
			loadError = 'Viewer element not available.';
			isLoading = false;
			return;
		}

		// Download the EPUB
		const fileName = basename(doc.path);
		try {
			const blob = await downloadFileFromSources(doc.sourceGuids, fileName);
			const arrayBuffer = await blob.arrayBuffer();

			// Parse the EPUB
			book = ePub(arrayBuffer);
			await (book as any).ready;

			// epub.js types don't expose .length but it exists at runtime
			totalSpine = (book as any).spine.length ?? 0;

			// Render into the viewer div
			const width = viewerEl.clientWidth;
			const height = viewerEl.clientHeight;
			rendition = book.renderTo(viewerEl, { width, height });

			// Apply initial font size
			rendition.themes.fontSize(`${fontSize}%`);

			// Subscribe to app theme changes; fires immediately so the correct
			// scheme colours are applied before rendition.display() renders.
			themeUnsubscribe = theme.subscribe(() => {
				applyEpubTheme(document.documentElement.classList.contains('dark'));
			});

			// Track location changes and debounce-save CFI progress
			rendition.on('relocated', (location: Location) => {
				if (location?.start?.index !== undefined) {
					spineIndex = location.start.index;
				}
				if (typeof location?.start?.percentage === 'number') {
					percentage = Math.round(location.start.percentage * 100);
				}
				if (location?.start?.cfi) {
					scheduleProgressSave(location.start.cfi, location.start.percentage ?? 0);
				}
			});

			// Forward keyboard events from inside the iframe
			rendition.on('keydown', handleKeydown);

			// Restore saved CFI position, or start from the beginning
			let startTarget: string | undefined;
			try {
				const saved = await fetchReadingState(fingerprint);
				if (saved?.position) {
					const parsed = JSON.parse(saved.position) as { cfi?: string };
					if (typeof parsed.cfi === 'string') startTarget = parsed.cfi;
				}
			} catch {
				// Silently ignore — start from beginning
			}
			await rendition.display(startTarget);
			renditionReady = true;
			isLoading = false;

			// Resize observer — re-fit when the viewer container changes size
			resizeObserver = new ResizeObserver((entries) => {
				const entry = entries[0];
				const { width: w, height: h } = entry.contentRect;
				if (resizeTimer) clearTimeout(resizeTimer);
				resizeTimer = setTimeout(() => rendition?.resize(w, h), RESIZE_DEBOUNCE_MS);
			});
			resizeObserver.observe(viewerEl);
		} catch (err) {
			loadError = err instanceof Error ? err.message : 'Failed to load EPUB.';
			isLoading = false;
		}

		resetToolbarTimer();
	});

	onDestroy(() => {
		themeUnsubscribe?.();
		if (toolbarTimer) clearTimeout(toolbarTimer);
		if (resizeTimer) clearTimeout(resizeTimer);
		if (progressTimer) clearTimeout(progressTimer);
		resizeObserver?.disconnect();
		rendition?.destroy();
		(book as any)?.destroy();
	});
</script>

<svelte:window onkeydown={handleKeydown} />

<svelte:head>
	<title>{doc ? basename(doc.path) : 'EPUB'} — Read Flow</title>
</svelte:head>

<!--
	Fills the reader layout's h-dvh container (bg-slate-900).
	Touch/swipe events on the outer wrapper complement keyboard events forwarded
	from inside the epub.js iframe via rendition.on('keydown', ...).
-->
<div
	class="relative flex flex-col h-full"
	ontouchstart={handleTouchStart}
	ontouchend={handleTouchEnd}
	role="application"
	aria-label="EPUB reader"
>
	<!-- ── Toolbar ─────────────────────────────────────────────── -->
	<header
		class="flex items-center gap-2 px-4 py-3 shrink-0 bg-slate-800/95 backdrop-blur-sm z-10
			transition-transform duration-200
			{toolbarVisible ? 'translate-y-0' : '-translate-y-full'} md:translate-y-0"
	>
		<a
			href="/documents/{fingerprint}"
			class="p-2 -ml-2 rounded-lg text-white hover:bg-slate-700 transition-colors shrink-0"
			aria-label="Back to document details"
		>
			<Icon name="arrow-left" class="w-5 h-5" />
		</a>

		<!-- Chapter info -->
		<div class="flex-1 min-w-0 flex flex-col">
			<span class="text-sm font-medium text-slate-200 truncate">
				{doc ? basename(doc.path) : 'Loading…'}
			</span>
			{#if !isLoading && totalSpine > 0}
				<span class="text-xs text-slate-500 tabular-nums">
					Ch. {spineIndex + 1} / {totalSpine}
				</span>
			{/if}
		</div>

		<!-- Font size controls -->
		{#if !isLoading && !loadError}
			<div class="flex items-center gap-0.5 shrink-0">
				<button
					onclick={() => void adjustFontSize(-FONT_SIZE_STEP)}
					disabled={fontSize <= FONT_SIZE_MIN}
					class="p-1.5 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors disabled:opacity-30"
					aria-label="Decrease font size"
				>
					<Icon name="minus" class="w-4 h-4" />
				</button>
				<span class="text-xs text-slate-400 w-11 text-center tabular-nums">{fontSize}%</span>
				<button
					onclick={() => void adjustFontSize(FONT_SIZE_STEP)}
					disabled={fontSize >= FONT_SIZE_MAX}
					class="p-1.5 rounded text-slate-400 hover:text-white hover:bg-slate-700 transition-colors disabled:opacity-30"
					aria-label="Increase font size"
				>
					<Icon name="plus" class="w-4 h-4" />
				</button>
			</div>
		{/if}
	</header>

	<!-- ── Viewer ──────────────────────────────────────────────── -->
	<!--
		epub.js renders an <iframe> directly inside this div.
		bg-white is intentional — the book content renders on a white canvas
		regardless of the overall app theme (same as most e-reader apps).
	-->
	<div
		bind:this={viewerEl}
		class="flex-1 overflow-hidden bg-white dark:bg-slate-800"
		onclick={handleViewerTap}
		role="presentation"
	>
		{#if isLoading}
			<div class="flex items-center justify-center gap-2.5 h-full text-slate-400">
				<Icon name="loader" class="w-6 h-6 animate-spin" />
				<span class="text-sm">Loading EPUB…</span>
			</div>
		{:else if loadError}
			<div class="flex flex-col items-center gap-3 h-full justify-center text-center px-8 max-w-sm mx-auto">
				<Icon name="alert-circle" class="w-10 h-10 text-red-400 shrink-0" />
				<p class="text-sm text-slate-600 leading-relaxed">{loadError}</p>
				<a
					href="/documents/{fingerprint}"
					class="text-sm text-slate-500 hover:text-slate-700 underline transition-colors"
				>
					Back to document
				</a>
			</div>
		{/if}
		<!-- epub.js appends its iframe here when loaded -->
	</div>

	<!-- ── Bottom navigation ────────────────────────────────────── -->
	<footer
		class="flex items-center justify-between px-6 shrink-0 bg-slate-800/95 backdrop-blur-sm
			transition-transform duration-200
			{toolbarVisible ? 'translate-y-0' : 'translate-y-full'} md:translate-y-0"
		style="padding-top: 0.75rem; padding-bottom: max(0.75rem, env(safe-area-inset-bottom, 0px))"
	>
		<button
			onclick={() => void navigate('prev')}
			disabled={!renditionReady}
			class="p-2 rounded-lg text-white hover:bg-slate-700 transition-colors disabled:opacity-30 min-w-[44px] min-h-[44px] flex items-center justify-center"
			aria-label="Previous"
		>
			<Icon name="arrow-left" class="w-5 h-5" />
		</button>

		<span class="text-sm text-slate-400 tabular-nums select-none">
			{#if !isLoading && !loadError}
				{percentage}%
			{:else}
				–
			{/if}
		</span>

		<button
			onclick={() => void navigate('next')}
			disabled={!renditionReady}
			class="p-2 rounded-lg text-white hover:bg-slate-700 transition-colors disabled:opacity-30 min-w-[44px] min-h-[44px] flex items-center justify-center"
			aria-label="Next"
		>
			<Icon name="chevron-down" class="w-5 h-5 -rotate-90" />
		</button>
	</footer>
</div>
