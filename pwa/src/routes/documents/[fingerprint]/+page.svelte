<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import Icon from '$lib/components/Icon.svelte';
	import DocumentDetail from '$lib/components/DocumentDetail.svelte';

	const fingerprint = $derived($page.params.fingerprint ?? '');
</script>

<svelte:head>
	<title>Document — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<a
		href="/library"
		class="inline-flex items-center gap-1.5 text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 mb-5 transition-colors"
	>
		<Icon name="arrow-left" class="w-4 h-4" />
		Library
	</a>

	<!-- The document ceases to exist when its last format is deleted — leave the page. -->
	<DocumentDetail {fingerprint} onclose={() => goto('/library')} />
</div>
