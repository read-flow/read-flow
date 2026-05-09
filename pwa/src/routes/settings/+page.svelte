<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import { theme, setTheme, isDarkScheme, type ColorScheme } from '$lib/stores/theme';

	interface SchemeEntry {
		id: ColorScheme;
		label: string;
		/** [page-bg, card-surface, primary-text] — shown as colour swatches */
		swatches: [string, string, string];
	}

	interface SchemeGroup {
		label: string;
		schemes: SchemeEntry[];
	}

	const groups: SchemeGroup[] = [
		{
			label: 'Automatic',
			schemes: [
				{ id: 'system', label: 'System', swatches: ['#f8fafc', '#ffffff', '#0f172a'] },
			],
		},
		{
			label: 'Slate (default)',
			schemes: [
				{ id: 'slate-light', label: 'Light', swatches: ['#f8fafc', '#ffffff', '#0f172a'] },
				{ id: 'slate-dark',  label: 'Dark',  swatches: ['#0f172a', '#1e293b', '#f1f5f9'] },
			],
		},
		{
			label: 'Nord',
			schemes: [
				{ id: 'nord-light', label: 'Light', swatches: ['#e5e9f0', '#eceff4', '#2e3440'] },
				{ id: 'nord-dark',  label: 'Dark',  swatches: ['#2e3440', '#3b4252', '#d8dee9'] },
			],
		},
		{
			label: 'Catppuccin',
			schemes: [
				{ id: 'catppuccin-latte',     label: 'Latte',     swatches: ['#eff1f5', '#e6e9ef', '#4c4f69'] },
				{ id: 'catppuccin-frappe',    label: 'Frappé',    swatches: ['#303446', '#414559', '#c6d0f5'] },
				{ id: 'catppuccin-macchiato', label: 'Macchiato', swatches: ['#24273a', '#363a4f', '#cad3f5'] },
				{ id: 'catppuccin-mocha',     label: 'Mocha',     swatches: ['#1e1e2e', '#313244', '#cdd6f4'] },
			],
		},
		{
			label: 'One',
			schemes: [
				{ id: 'one-light', label: 'Light', swatches: ['#fafafa', '#f0f0f0', '#383a42'] },
				{ id: 'one-dark',  label: 'Dark',  swatches: ['#21252b', '#282c34', '#abb2bf'] },
			],
		},
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

	<!-- Colour scheme picker -->
	<section>
		<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-4">
			Appearance
		</h2>

		<div class="space-y-5">
			{#each groups as group}
				<div>
					<p class="text-xs font-medium text-slate-500 dark:text-slate-400 mb-2">{group.label}</p>

					<div class="grid gap-2" style="grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr))">
						{#each group.schemes as scheme}
							{@const active = $theme === scheme.id}
							<button
								onclick={() => setTheme(scheme.id)}
								class="relative flex flex-col items-center gap-2 px-3 py-3 rounded-xl border text-sm
									transition-colors text-left
									{active
										? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700/60'
										: 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/40'}"
							>
								<!-- Colour swatches -->
								<div class="flex gap-1 items-center">
									{#each scheme.swatches as color}
										<span
											class="block w-4 h-4 rounded-full border border-black/10 dark:border-white/10 shrink-0"
											style="background:{color}"
										></span>
									{/each}
								</div>

								<!-- Label + dark/light badge -->
								<div class="flex items-center gap-1.5 w-full justify-center">
									<span class="font-medium text-xs text-slate-800 dark:text-slate-200 truncate">
										{scheme.label}
									</span>
									{#if scheme.id !== 'system'}
										<Icon
											name={isDarkScheme(scheme.id) ? 'moon' : 'sun'}
											class="w-3 h-3 shrink-0 text-slate-400 dark:text-slate-500"
										/>
									{:else}
										<Icon name="monitor" class="w-3 h-3 shrink-0 text-slate-400 dark:text-slate-500" />
									{/if}
								</div>

								<!-- Selected tick -->
								{#if active}
									<span class="absolute top-1.5 right-1.5">
										<Icon name="check" class="w-3 h-3 text-slate-900 dark:text-slate-200" />
									</span>
								{/if}
							</button>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	</section>
</div>
