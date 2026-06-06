<script lang="ts">
	import { fetchCoverFromSources, fetchDocumentCoverFromSources } from '$lib/api/aggregator';
	import Icon from '$lib/components/Icon.svelte';

	interface Props {
		sourceGuids: Record<number, string>;
		hasCover: boolean;
		/** When provided, fetches the document's selected cover via GET /documents/<guid>/cover. */
		documentGuid?: string;
		alt?: string;
		class?: string;
	}

	let {
		sourceGuids,
		hasCover,
		documentGuid,
		alt = 'Cover',
		class: className = '',
	}: Props = $props();

	let objectUrl = $state<string | null>(null);
	let loading = $state(false);
	let element: HTMLDivElement | undefined = $state();

	$effect(() => {
		// Depend on the cover's identity so switching documents (which reuses
		// this component instance) clears the stale thumbnail and reloads.
		const coverKey = documentGuid ?? Object.values(sourceGuids).join(',');
		void coverKey;

		// Clear the previous document's image (the cleanup of the prior run
		// revokes its URL). Done before the guard so a new file without a cover
		// doesn't keep showing the old thumbnail.
		objectUrl = null;

		if (!hasCover || !element) return;

		// Holds the URL fetched for *this* effect run, so cleanup revokes the
		// right one without re-triggering the effect by reading reactive state.
		let currentUrl: string | null = null;

		const observer = new IntersectionObserver(
			(entries) => {
				if (!entries[0]?.isIntersecting) return;
				observer.disconnect();
				void (async () => {
					loading = true;
					try {
						const url = documentGuid
							? await fetchDocumentCoverFromSources(documentGuid, sourceGuids)
							: await fetchCoverFromSources(sourceGuids);
						currentUrl = url;
						objectUrl = url;
					} finally {
						loading = false;
					}
				})();
			},
			{ rootMargin: '200px' },
		);
		observer.observe(element);

		return () => {
			observer.disconnect();
			if (currentUrl) URL.revokeObjectURL(currentUrl);
		};
	});
</script>

<div bind:this={element} class="flex items-center justify-center bg-slate-100 dark:bg-slate-800 overflow-hidden {className}">
	{#if objectUrl}
		<img src={objectUrl} {alt} class="w-full h-full object-cover" />
	{:else if loading}
		<Icon name="loader" class="w-5 h-5 text-slate-400 animate-spin" />
	{:else}
		<Icon name="library" class="w-5 h-5 text-slate-400" />
	{/if}
</div>
