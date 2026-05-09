<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
</script>

<svelte:head>
	<title>Settings — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<h1 class="text-xl font-semibold text-slate-900 mb-6">Settings</h1>

	<nav class="space-y-2">
		<a
			href="/settings/sources"
			class="flex items-center justify-between px-4 py-3.5 rounded-xl border border-slate-200 bg-white hover:bg-slate-50 transition-colors group"
		>
			<div class="flex items-center gap-3">
				<div class="w-9 h-9 rounded-lg bg-slate-100 flex items-center justify-center">
					<Icon name="server" class="w-5 h-5 text-slate-600" />
				</div>
				<div>
					<p class="text-sm font-medium text-slate-900">Sources</p>
					<p class="text-xs text-slate-400 mt-0.5">Manage remote read-flow servers</p>
				</div>
			</div>
			<Icon name="chevron-down" class="w-4 h-4 text-slate-300 -rotate-90 group-hover:text-slate-500 transition-colors" />
		</a>
	</nav>
</div>
