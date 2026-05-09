<script lang="ts">
	import { page } from '$app/stores';
	import Icon from '$lib/components/Icon.svelte';

	const fingerprint = $derived($page.params.fingerprint ?? '');
</script>

<svelte:head>
	<title>Reading — Read Flow</title>
</svelte:head>

<!--
	Full-screen EPUB reader — placeholder.
	Implementation will mount epub.js into #epub-container and wire up:
	  - CFI-based location tracking + debounced progress sync
	  - Toolbar auto-hide on mobile (3 s inactivity)
	  - Swipe left/right for page turns on touch devices
	  - Font size and theme preferences persisted to IndexedDB
-->
<div class="relative flex flex-col h-full text-white">
	<!-- Toolbar -->
	<header class="flex items-center gap-3 px-4 py-3 bg-slate-800/90 backdrop-blur-sm z-10">
		<a
			href="/documents/{fingerprint}"
			class="p-2 -ml-2 rounded-lg hover:bg-slate-700 transition-colors"
			aria-label="Back"
		>
			<Icon name="arrow-left" class="w-5 h-5" />
		</a>
		<span class="flex-1 text-sm font-medium text-slate-200 truncate">EPUB Reader</span>
		<span class="text-xs text-slate-400 font-mono">{fingerprint.slice(0, 8)}…</span>
	</header>

	<!-- Reader container (epub.js will render here) -->
	<div id="epub-container" class="flex-1 bg-white flex items-center justify-center">
		<div class="text-center px-6">
			<Icon name="library" class="w-12 h-12 text-slate-200 mx-auto mb-4" />
			<p class="text-slate-400 text-sm">EPUB reader coming soon</p>
			<p class="text-slate-500 text-xs mt-1">Will use epub.js with CFI progress tracking</p>
		</div>
	</div>

	<!-- Bottom controls -->
	<footer
		class="flex items-center justify-between px-6 py-3 bg-slate-800/90 backdrop-blur-sm"
		style="padding-bottom: max(0.75rem, env(safe-area-inset-bottom))"
	>
		<button class="p-2 rounded-lg hover:bg-slate-700 transition-colors" aria-label="Previous page">
			<Icon name="arrow-left" class="w-5 h-5" />
		</button>
		<span class="text-xs text-slate-400">Page — / —</span>
		<button class="p-2 rounded-lg hover:bg-slate-700 transition-colors" aria-label="Next page">
			<Icon name="chevron-down" class="w-5 h-5 -rotate-90" />
		</button>
	</footer>
</div>
