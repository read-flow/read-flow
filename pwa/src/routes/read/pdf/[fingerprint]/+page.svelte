<script lang="ts">
	import { page } from '$app/stores';
	import Icon from '$lib/components/Icon.svelte';

	const fingerprint = $derived($page.params.fingerprint ?? '');
	let currentPage = $state(1);
	let totalPages = $state(0);
</script>

<svelte:head>
	<title>Reading — Read Flow</title>
</svelte:head>

<!--
	Full-screen PDF reader — placeholder.
	Implementation will initialise pdfjs-dist into #pdf-container and wire up:
	  - Page-number-based progress tracking + debounced sync to all sources
	  - Pinch-to-zoom on touch devices
	  - Keyboard navigation (arrow keys, PgUp/PgDn, Home/End)
	  - Zoom controls persisted to IndexedDB preferences
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
		<span class="flex-1 text-sm font-medium text-slate-200 truncate">PDF Reader</span>
		<span class="hidden md:block text-xs text-slate-400">Zoom: 100%</span>
	</header>

	<!-- PDF container (pdfjs-dist will render here) -->
	<div id="pdf-container" class="flex-1 overflow-auto bg-slate-700 flex items-center justify-center">
		<div class="text-center px-6">
			<Icon name="library" class="w-12 h-12 text-slate-400 mx-auto mb-4" />
			<p class="text-slate-300 text-sm">PDF reader coming soon</p>
			<p class="text-slate-500 text-xs mt-1">Will use pdfjs-dist with page-based progress tracking</p>
		</div>
	</div>

	<!-- Bottom controls -->
	<footer
		class="flex items-center justify-between px-6 py-3 bg-slate-800/90 backdrop-blur-sm"
		style="padding-bottom: max(0.75rem, env(safe-area-inset-bottom))"
	>
		<button
			onclick={() => currentPage = Math.max(1, currentPage - 1)}
			class="p-2 rounded-lg hover:bg-slate-700 transition-colors"
			aria-label="Previous page"
		>
			<Icon name="arrow-left" class="w-5 h-5" />
		</button>

		<span class="text-xs text-slate-400">
			{#if totalPages > 0}
				Page {currentPage} / {totalPages}
			{:else}
				Page — / —
			{/if}
		</span>

		<button
			onclick={() => currentPage = totalPages ? Math.min(totalPages, currentPage + 1) : currentPage}
			class="p-2 rounded-lg hover:bg-slate-700 transition-colors"
			aria-label="Next page"
		>
			<Icon name="chevron-down" class="w-5 h-5 -rotate-90" />
		</button>
	</footer>
</div>
