<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { sources, loadSources } from '$lib/stores/sources';
	import { ReadFlowClient, type ScanSummary, type CheckMissingResponse } from '$lib/api/client';

	let selectedId = $state<number | null>(null);

	const selectedSource = $derived($sources.find((s) => s.id === selectedId) ?? null);
	const client = $derived(selectedSource ? new ReadFlowClient(selectedSource) : null);

	onMount(async () => {
		await loadSources();
		selectedId = $sources[0]?.id ?? null;
	});

	// ── Maintenance: scan + check-missing ──────────────────────────────────────
	let scanning = $state(false);
	let scanResult = $state<ScanSummary | null>(null);
	let scanError = $state('');

	async function runScan() {
		if (!client || scanning) return;
		scanning = true;
		scanError = '';
		scanResult = null;
		try {
			scanResult = await client.scan();
		} catch (err) {
			scanError = err instanceof Error ? err.message : 'Scan failed.';
		} finally {
			scanning = false;
		}
	}

	let purge = $state(false);
	let checking = $state(false);
	let missingResult = $state<CheckMissingResponse | null>(null);
	let missingError = $state('');

	async function runCheckMissing() {
		if (!client || checking) return;
		checking = true;
		missingError = '';
		missingResult = null;
		try {
			missingResult = await client.checkMissing(purge);
		} catch (err) {
			missingError = err instanceof Error ? err.message : 'Check failed.';
		} finally {
			checking = false;
		}
	}
</script>

<svelte:head>
	<title>Server admin — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<a
		href="/settings"
		class="md:hidden inline-flex items-center gap-1.5 text-sm text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 mb-4 transition-colors"
	>
		<Icon name="arrow-left" class="w-4 h-4" />
		Settings
	</a>

	<h1 class="text-xl font-semibold mb-1">Server admin</h1>
	<p class="text-sm text-slate-400 dark:text-slate-500 mb-6">
		Manage a server. Requires the <span class="font-medium">owner</span> role on the selected source.
	</p>

	{#if $sources.length === 0}
		<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 px-4 py-6 text-center">
			<p class="text-sm text-slate-500 dark:text-slate-400">No sources configured.</p>
			<a href="/settings/sources" class="mt-2 inline-block text-sm text-accent underline underline-offset-2">Add a source</a>
		</div>
	{:else}
		<!-- Source picker -->
		<label class="block mb-6">
			<span class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">Server</span>
			<select
				bind:value={selectedId}
				class="mt-1.5 w-full rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
			>
				{#each $sources as s}
					<option value={s.id}>{s.name}</option>
				{/each}
			</select>
		</label>

		<!-- Maintenance -->
		<section class="mb-8">
			<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-2">Maintenance</h2>
			<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50">
				<!-- Scan -->
				<div class="px-4 py-3">
					<div class="flex items-center justify-between gap-3">
						<div class="min-w-0">
							<p class="text-sm font-medium">Scan library</p>
							<p class="text-xs text-slate-400 dark:text-slate-500">Scan all configured directories on this server.</p>
						</div>
						<button
							onclick={runScan}
							disabled={scanning || !client}
							class="shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
						>
							{#if scanning}<Icon name="loader" class="w-4 h-4 animate-spin" />{/if}
							{scanning ? 'Scanning…' : 'Scan'}
						</button>
					</div>
					{#if scanResult}
						<p class="mt-2 text-xs text-slate-500 dark:text-slate-400">
							Discovered {scanResult.discovered}, processed {scanResult.processed}, errors {scanResult.errors}.
						</p>
					{/if}
					{#if scanError}<p class="mt-2 text-xs text-red-500 dark:text-red-400">{scanError}</p>{/if}
				</div>

				<!-- Check missing -->
				<div class="px-4 py-3">
					<div class="flex items-center justify-between gap-3">
						<div class="min-w-0">
							<p class="text-sm font-medium">Check missing files</p>
							<p class="text-xs text-slate-400 dark:text-slate-500">Find database records whose file no longer exists on disk.</p>
						</div>
						<div class="flex items-center gap-3 shrink-0">
							<label class="flex items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400">
								<input type="checkbox" bind:checked={purge} class="accent-red-600" />
								Purge
							</label>
							<button
								onclick={runCheckMissing}
								disabled={checking || !client}
								class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 text-sm font-medium hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
							>
								{#if checking}<Icon name="loader" class="w-4 h-4 animate-spin" />{/if}
								{checking ? 'Checking…' : 'Check'}
							</button>
						</div>
					</div>
					{#if missingResult}
						{#if missingResult.missing.length === 0}
							<p class="mt-2 text-xs text-green-600 dark:text-green-400">All files present.</p>
						{:else}
							<p class="mt-2 text-xs text-slate-500 dark:text-slate-400">
								{missingResult.missing.length} missing{missingResult.purged ? ' (purged)' : ''}:
							</p>
							<ul class="mt-1 max-h-40 overflow-y-auto text-xs text-slate-400 dark:text-slate-500 font-mono space-y-0.5">
								{#each missingResult.missing as path}<li class="truncate" title={path}>{path}</li>{/each}
							</ul>
						{/if}
					{/if}
					{#if missingError}<p class="mt-2 text-xs text-red-500 dark:text-red-400">{missingError}</p>{/if}
				</div>
			</div>
		</section>
	{/if}
</div>
