<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { theme, setTheme, type Theme } from '$lib/stores/theme';

	const themeOptions: { value: Theme; label: string; icon: 'monitor' | 'sun' | 'moon'; description: string }[] = [
		{ value: 'system', label: 'System', icon: 'monitor', description: 'Follow OS preference' },
		{ value: 'light', label: 'Light', icon: 'sun', description: 'Always light' },
		{ value: 'dark', label: 'Dark', icon: 'moon', description: 'Always dark' },
	];
</script>

<svelte:head>
	<title>Settings — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<h1 class="text-xl font-semibold text-slate-900 dark:text-slate-100 mb-6">Settings</h1>

	<!-- Sources link -->
	<nav class="space-y-2 mb-8">
		<a
			href="/settings/sources"
			class="flex items-center justify-between px-4 py-3.5 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:bg-slate-50 dark:hover:bg-slate-700/50 transition-colors group"
		>
			<div class="flex items-center gap-3">
				<div class="w-9 h-9 rounded-lg bg-slate-100 dark:bg-slate-700 flex items-center justify-center">
					<Icon name="server" class="w-5 h-5 text-slate-600 dark:text-slate-400" />
				</div>
				<div>
					<p class="text-sm font-medium text-slate-900 dark:text-slate-100">Sources</p>
					<p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5">Manage remote read-flow servers</p>
				</div>
			</div>
			<Icon name="chevron-down" class="w-4 h-4 text-slate-300 dark:text-slate-600 -rotate-90 group-hover:text-slate-500 dark:group-hover:text-slate-400 transition-colors" />
		</a>
	</nav>

	<!-- Appearance -->
	<section>
		<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-3">Appearance</h2>

		<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 p-4">
			<p class="text-sm font-medium text-slate-900 dark:text-slate-100 mb-3">Theme</p>
			<div class="grid grid-cols-3 gap-2">
				{#each themeOptions as opt}
					<button
						onclick={() => setTheme(opt.value)}
						class="flex flex-col items-center gap-2 px-3 py-3 rounded-lg border text-sm transition-colors
							{$theme === opt.value
								? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
								: 'border-slate-200 dark:border-slate-700 text-slate-500 dark:text-slate-400 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/50'}"
					>
						<Icon name={opt.icon} class="w-5 h-5" />
						<span class="font-medium">{opt.label}</span>
						<span class="text-xs text-slate-400 dark:text-slate-500 hidden sm:block">{opt.description}</span>
					</button>
				{/each}
			</div>
		</div>
	</section>
</div>
