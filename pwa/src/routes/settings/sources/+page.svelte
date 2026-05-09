<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { sources, loadSources, addSource, removeSource, moveSource } from '$lib/stores/sources';

	let showForm = $state(false);
	let isSubmitting = $state(false);
	let submitError = $state<string | null>(null);

	let name = $state('');
	let baseUrl = $state('');
	let userId = $state('');
	let passphrase = $state('');

	onMount(loadSources);

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		isSubmitting = true;
		submitError = null;

		const result = await addSource({ name, baseUrl, userId, passphrase });
		isSubmitting = false;

		if (result.ok) {
			name = '';
			baseUrl = '';
			userId = '';
			passphrase = '';
			showForm = false;
		} else {
			submitError = result.error;
		}
	}

	async function handleRemove(id: number) {
		if (!confirm('Remove this source?')) return;
		await removeSource(id);
	}
</script>

<svelte:head>
	<title>Sources — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<!-- Back link (mobile) -->
	<a
		href="/settings"
		class="md:hidden inline-flex items-center gap-1.5 text-sm text-slate-500 hover:text-slate-800 mb-4 transition-colors"
	>
		<Icon name="arrow-left" class="w-4 h-4" />
		Settings
	</a>

	<div class="flex items-center justify-between mb-6">
		<h1 class="text-xl font-semibold text-slate-900">Sources</h1>
		<button
			onclick={() => { showForm = !showForm; submitError = null; }}
			class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 text-white text-sm font-medium hover:bg-slate-700 transition-colors"
		>
			<Icon name="plus" class="w-4 h-4" />
			Add source
		</button>
	</div>

	<!-- Add source form -->
	{#if showForm}
		<form
			onsubmit={handleSubmit}
			class="mb-6 p-4 rounded-xl border border-slate-200 bg-white space-y-3"
		>
			<h2 class="text-sm font-medium text-slate-900">New source</h2>

			<div class="space-y-3">
				<div>
					<label for="source-name" class="block text-xs font-medium text-slate-600 mb-1">Name</label>
					<input
						id="source-name"
						type="text"
						bind:value={name}
						placeholder="My home server"
						required
						class="w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50
							focus:outline-none focus:ring-2 focus:ring-slate-300 focus:border-transparent
							placeholder:text-slate-300 text-slate-900"
					/>
				</div>

				<div>
					<label for="source-url" class="block text-xs font-medium text-slate-600 mb-1">Base URL</label>
					<input
						id="source-url"
						type="url"
						bind:value={baseUrl}
						placeholder="http://192.168.1.10:8000"
						required
						class="w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50
							focus:outline-none focus:ring-2 focus:ring-slate-300 focus:border-transparent
							placeholder:text-slate-300 text-slate-900"
					/>
				</div>

				<div class="grid grid-cols-2 gap-3">
					<div>
						<label for="source-user" class="block text-xs font-medium text-slate-600 mb-1">User ID</label>
						<input
							id="source-user"
							type="text"
							bind:value={userId}
							placeholder="alice"
							required
							autocomplete="username"
							class="w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50
								focus:outline-none focus:ring-2 focus:ring-slate-300 focus:border-transparent
								placeholder:text-slate-300 text-slate-900"
						/>
					</div>
					<div>
						<label for="source-pass" class="block text-xs font-medium text-slate-600 mb-1">Passphrase</label>
						<input
							id="source-pass"
							type="password"
							bind:value={passphrase}
							required
							autocomplete="current-password"
							class="w-full px-3 py-2 rounded-lg border border-slate-200 bg-slate-50
								focus:outline-none focus:ring-2 focus:ring-slate-300 focus:border-transparent
								text-slate-900"
						/>
					</div>
				</div>
			</div>

			{#if submitError}
				<div class="flex items-start gap-2 p-3 rounded-lg bg-red-50 border border-red-100">
					<Icon name="alert-circle" class="w-4 h-4 text-red-500 shrink-0 mt-0.5" />
					<p class="text-sm text-red-700">{submitError}</p>
				</div>
			{/if}

			<div class="flex justify-end gap-2 pt-1">
				<button
					type="button"
					onclick={() => { showForm = false; submitError = null; }}
					class="px-3 py-1.5 rounded-lg text-sm text-slate-600 hover:bg-slate-100 transition-colors"
				>
					Cancel
				</button>
				<button
					type="submit"
					disabled={isSubmitting}
					class="inline-flex items-center gap-1.5 px-4 py-1.5 rounded-lg bg-slate-900 text-white text-sm font-medium
						hover:bg-slate-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
				>
					{#if isSubmitting}
						<Icon name="loader" class="w-4 h-4 animate-spin" />
						Connecting…
					{:else}
						<Icon name="check" class="w-4 h-4" />
						Add
					{/if}
				</button>
			</div>
		</form>
	{/if}

	<!-- Source list -->
	{#if $sources.length === 0}
		<div class="flex flex-col items-center gap-3 py-16 text-center">
			<Icon name="server" class="w-10 h-10 text-slate-200" />
			<p class="text-sm text-slate-400">No sources yet. Add a remote read-flow server above.</p>
		</div>
	{:else}
		<ul class="space-y-2">
			{#each $sources as source (source.id)}
				<li class="flex items-center gap-3 px-4 py-3 rounded-xl border border-slate-200 bg-white">
					<Icon name="server" class="w-5 h-5 text-slate-400 shrink-0" />

					<div class="flex-1 min-w-0">
						<p class="text-sm font-medium text-slate-900 truncate">{source.name}</p>
						<p class="text-xs text-slate-400 truncate">{source.baseUrl}</p>
					</div>

					<div class="flex items-center gap-1 shrink-0">
						<button
							onclick={() => moveSource(source.id!, 'up')}
							aria-label="Move up"
							class="p-1.5 rounded text-slate-400 hover:text-slate-700 hover:bg-slate-100 transition-colors"
						>
							<Icon name="chevron-up" class="w-4 h-4" />
						</button>
						<button
							onclick={() => moveSource(source.id!, 'down')}
							aria-label="Move down"
							class="p-1.5 rounded text-slate-400 hover:text-slate-700 hover:bg-slate-100 transition-colors"
						>
							<Icon name="chevron-down" class="w-4 h-4" />
						</button>
						<button
							onclick={() => handleRemove(source.id!)}
							aria-label="Remove source"
							class="p-1.5 rounded text-slate-400 hover:text-red-600 hover:bg-red-50 transition-colors"
						>
							<Icon name="trash" class="w-4 h-4" />
						</button>
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>
