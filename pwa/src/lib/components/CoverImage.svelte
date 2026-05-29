<script lang="ts">
	import { onDestroy } from 'svelte';
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

	let observer: IntersectionObserver | null = null;

	$effect(() => {
		if (!hasCover || !element) return;

		observer = new IntersectionObserver(
			(entries) => {
				if (entries[0]?.isIntersecting) {
					observer?.disconnect();
					loadCover();
				}
			},
			{ rootMargin: '200px' },
		);
		observer.observe(element);

		return () => observer?.disconnect();
	});

	async function loadCover(): Promise<void> {
		loading = true;
		try {
			objectUrl = documentGuid
				? await fetchDocumentCoverFromSources(documentGuid, sourceGuids)
				: await fetchCoverFromSources(sourceGuids);
		} finally {
			loading = false;
		}
	}

	onDestroy(() => {
		observer?.disconnect();
		if (objectUrl) URL.revokeObjectURL(objectUrl);
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
