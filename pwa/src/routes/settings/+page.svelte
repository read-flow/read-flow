<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
</script>

<svelte:head>
	<title>Settings — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<h1 class="text-xl font-semibold mb-6">Settings</h1>

	<nav class="space-y-2">
		<a
			href="/settings/sources"
			class="flex items-center justify-between px-4 py-3.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors group"
		>
			<div class="flex items-center gap-3">
				<div class="w-9 h-9 rounded-lg bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
					<Icon name="server" class="w-5 h-5 text-slate-600 dark:text-slate-400" />
				</div>
				<div>
					<p class="text-sm font-medium">Sources</p>
					<p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5">Manage remote read-flow servers</p>
				</div>
			</div>
			<Icon name="chevron-down" class="w-4 h-4 text-slate-300 dark:text-slate-600 -rotate-90 group-hover:text-slate-500 dark:group-hover:text-slate-400 transition-colors" />
		</a>

		<a
			href="/settings/admin"
			class="flex items-center justify-between px-4 py-3.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors group"
		>
			<div class="flex items-center gap-3">
				<div class="w-9 h-9 rounded-lg bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
					<Icon name="settings" class="w-5 h-5 text-slate-600 dark:text-slate-400" />
				</div>
				<div>
					<p class="text-sm font-medium">Server admin</p>
					<p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5">Scan, settings, users (owner only)</p>
				</div>
			</div>
			<Icon name="chevron-down" class="w-4 h-4 text-slate-300 dark:text-slate-600 -rotate-90 group-hover:text-slate-500 dark:group-hover:text-slate-400 transition-colors" />
		</a>

		<a
			href="/settings/theme"
			class="flex items-center justify-between px-4 py-3.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors group"
		>
			<div class="flex items-center gap-3">
				<div class="w-9 h-9 rounded-lg bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
					<Icon name="sun" class="w-5 h-5 text-slate-600 dark:text-slate-400" />
				</div>
				<div>
					<p class="text-sm font-medium">Theme</p>
					<p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5">Modes, color schemes, custom colors</p>
				</div>
			</div>
			<Icon name="chevron-down" class="w-4 h-4 text-slate-300 dark:text-slate-600 -rotate-90 group-hover:text-slate-500 dark:group-hover:text-slate-400 transition-colors" />
		</a>
	</nav>
</div>
