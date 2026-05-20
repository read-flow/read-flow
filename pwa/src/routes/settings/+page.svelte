<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import {
		mode, lightScheme, darkScheme, customColors,
		setMode, setLightScheme, setDarkScheme, setCustomColors,
		isCustomDark, modeIcon, modeLabel,
		type Mode, type LightScheme, type DarkScheme, type CustomColors,
	} from '$lib/stores/theme';

	interface SchemeEntry<T extends string> {
		id: T;
		label: string;
		/** [page-bg, card-surface, primary-text] */
		swatches: [string, string, string];
	}

	const MODES: Mode[] = ['system', 'light', 'dark'];

	const lightSchemes: SchemeEntry<LightScheme>[] = [
		{ id: 'slate-light',       label: 'Slate',      swatches: ['#f8fafc', '#ffffff', '#0f172a'] },
		{ id: 'nord-light',        label: 'Nord',       swatches: ['#e5e9f0', '#eceff4', '#2e3440'] },
		{ id: 'catppuccin-latte',  label: 'Latte',      swatches: ['#eff1f5', '#e6e9ef', '#4c4f69'] },
		{ id: 'one-light',         label: 'One Light',  swatches: ['#fafafa', '#f0f0f0', '#383a42'] },
	];

	const darkSchemes: SchemeEntry<DarkScheme>[] = [
		{ id: 'slate-dark',           label: 'Slate',      swatches: ['#0f172a', '#1e293b', '#f1f5f9'] },
		{ id: 'nord-dark',            label: 'Nord',       swatches: ['#2e3440', '#3b4252', '#d8dee9'] },
		{ id: 'catppuccin-frappe',    label: 'Frappé',     swatches: ['#303446', '#414559', '#c6d0f5'] },
		{ id: 'catppuccin-macchiato', label: 'Macchiato',  swatches: ['#24273a', '#363a4f', '#cad3f5'] },
		{ id: 'catppuccin-mocha',     label: 'Mocha',      swatches: ['#1e1e2e', '#313244', '#cdd6f4'] },
		{ id: 'one-dark',             label: 'One Dark',   swatches: ['#21252b', '#282c34', '#abb2bf'] },
	];

	// ── Custom theme editor ─────────────────────────────────────────────────

	let draft = $state<CustomColors>({ ...$customColors });

	// Whether the custom scheme is currently active
	const customActive = $derived(
		$lightScheme === 'custom-light' || $darkScheme === 'custom-dark',
	);

	// Which section (light/dark) the custom card belongs to — tracks draft colors
	const customIsDark = $derived(isCustomDark(draft));

	function applyCustomTheme() {
		setCustomColors(draft);
	}
</script>

<svelte:head>
	<title>Settings — Read Flow</title>
</svelte:head>

<div class="max-w-2xl mx-auto px-4 py-6 md:px-6">
	<h1 class="text-xl font-semibold mb-6">Settings</h1>

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
					<p class="text-sm font-medium">Sources</p>
					<p class="text-xs text-slate-400 dark:text-slate-500 mt-0.5">Manage remote read-flow servers</p>
				</div>
			</div>
			<Icon name="chevron-down" class="w-4 h-4 text-slate-300 dark:text-slate-600 -rotate-90 group-hover:text-slate-500 dark:group-hover:text-slate-400 transition-colors" />
		</a>
	</nav>

	<!-- Appearance -->
	<section class="space-y-8">
		<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500">
			Appearance
		</h2>

		<!-- Mode selector -->
		<div>
			<p class="text-xs font-medium text-slate-500 dark:text-slate-400 mb-3">Mode</p>
			<div class="flex gap-2">
				{#each MODES as m}
					{@const active = $mode === m}
					<button
						onclick={() => setMode(m)}
						class="flex-1 flex items-center justify-center gap-2 py-2.5 rounded-xl border text-sm font-medium transition-colors
							{active
								? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700/60'
								: 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-400 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/40'}"
					>
						<Icon name={modeIcon(m)} class="w-4 h-4 shrink-0" />
						<span>{modeLabel(m)}</span>
					</button>
				{/each}
			</div>
		</div>

		<!-- Light scheme picker -->
		<div>
			<p class="text-xs font-medium text-slate-500 dark:text-slate-400 mb-3">
				<Icon name="sun" class="w-3.5 h-3.5 inline -mt-0.5 mr-1" />
				Light theme
			</p>
			<div class="grid gap-2" style="grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr))">
				{#each lightSchemes as scheme}
					{@const active = $lightScheme === scheme.id}
					<button
						onclick={() => setLightScheme(scheme.id)}
						class="relative flex flex-col items-center gap-2 px-3 py-3 rounded-xl border text-sm transition-colors
							{active
								? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700/60'
								: 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/40'}"
					>
						<div class="flex gap-1 items-center">
							{#each scheme.swatches as color}
								<span
									class="block w-4 h-4 rounded-full border border-black/10 dark:border-white/10 shrink-0"
									style="background:{color}"
								></span>
							{/each}
						</div>
						<span class="font-medium text-xs text-slate-800 dark:text-slate-200 truncate">
							{scheme.label}
						</span>
						{#if active}
							<span class="absolute top-1.5 right-1.5">
								<Icon name="check" class="w-3 h-3 text-slate-900 dark:text-slate-200" />
							</span>
						{/if}
					</button>
				{/each}

				<!-- Custom card — shown here when draft colors are light -->
				{#if !customIsDark}
					{@const active = $lightScheme === 'custom-light'}
					<button
						onclick={applyCustomTheme}
						class="relative flex flex-col items-center gap-2 px-3 py-3 rounded-xl border text-sm transition-colors
							{active
								? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700/60'
								: 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/40'}"
					>
						<div class="flex gap-1 items-center">
							{#each [draft.bg, draft.surface, draft.accent] as color}
								<span
									class="block w-4 h-4 rounded-full border border-black/10 dark:border-white/10 shrink-0"
									style="background:{color}"
								></span>
							{/each}
						</div>
						<span class="font-medium text-xs text-slate-800 dark:text-slate-200 truncate">Custom</span>
						{#if active}
							<span class="absolute top-1.5 right-1.5">
								<Icon name="check" class="w-3 h-3 text-slate-900 dark:text-slate-200" />
							</span>
						{/if}
					</button>
				{/if}
			</div>
		</div>

		<!-- Dark scheme picker -->
		<div>
			<p class="text-xs font-medium text-slate-500 dark:text-slate-400 mb-3">
				<Icon name="moon" class="w-3.5 h-3.5 inline -mt-0.5 mr-1" />
				Dark theme
			</p>
			<div class="grid gap-2" style="grid-template-columns: repeat(auto-fill, minmax(7rem, 1fr))">
				{#each darkSchemes as scheme}
					{@const active = $darkScheme === scheme.id}
					<button
						onclick={() => setDarkScheme(scheme.id)}
						class="relative flex flex-col items-center gap-2 px-3 py-3 rounded-xl border text-sm transition-colors
							{active
								? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700/60'
								: 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/40'}"
					>
						<div class="flex gap-1 items-center">
							{#each scheme.swatches as color}
								<span
									class="block w-4 h-4 rounded-full border border-black/10 dark:border-white/10 shrink-0"
									style="background:{color}"
								></span>
							{/each}
						</div>
						<span class="font-medium text-xs text-slate-800 dark:text-slate-200 truncate">
							{scheme.label}
						</span>
						{#if active}
							<span class="absolute top-1.5 right-1.5">
								<Icon name="check" class="w-3 h-3 text-slate-900 dark:text-slate-200" />
							</span>
						{/if}
					</button>
				{/each}

				<!-- Custom card — shown here when draft colors are dark -->
				{#if customIsDark}
					{@const active = $darkScheme === 'custom-dark'}
					<button
						onclick={applyCustomTheme}
						class="relative flex flex-col items-center gap-2 px-3 py-3 rounded-xl border text-sm transition-colors
							{active
								? 'border-slate-900 dark:border-slate-300 bg-slate-50 dark:bg-slate-700/60'
								: 'border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 hover:border-slate-300 dark:hover:border-slate-600 hover:bg-slate-50 dark:hover:bg-slate-700/40'}"
					>
						<div class="flex gap-1 items-center">
							{#each [draft.bg, draft.surface, draft.accent] as color}
								<span
									class="block w-4 h-4 rounded-full border border-black/10 dark:border-white/10 shrink-0"
									style="background:{color}"
								></span>
							{/each}
						</div>
						<span class="font-medium text-xs text-slate-800 dark:text-slate-200 truncate">Custom</span>
						{#if active}
							<span class="absolute top-1.5 right-1.5">
								<Icon name="check" class="w-3 h-3 text-slate-900 dark:text-slate-200" />
							</span>
						{/if}
					</button>
				{/if}
			</div>
		</div>

		<!-- Custom theme editor -->
		<div>
			<p class="text-xs font-medium text-slate-500 dark:text-slate-400 mb-3">
				Custom theme colors
			</p>
			<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 p-4 space-y-4">
				<div class="grid grid-cols-3 gap-3">
					{#each [
						{ key: 'bg',      label: 'Background' },
						{ key: 'surface', label: 'Container'  },
						{ key: 'accent',  label: 'Accent'     },
					] as field}
						<label class="flex flex-col items-center gap-2">
							<span class="text-xs font-medium text-slate-600 dark:text-slate-400">{field.label}</span>
							<input
								type="color"
								value={draft[field.key as keyof CustomColors]}
								oninput={(e) => {
									draft = { ...draft, [field.key]: (e.target as HTMLInputElement).value };
								}}
								class="w-full h-10 rounded-lg cursor-pointer border border-slate-200 dark:border-slate-600 p-0.5 bg-transparent"
							/>
							<input
								type="text"
								value={draft[field.key as keyof CustomColors]}
								oninput={(e) => {
									const raw = (e.target as HTMLInputElement).value.trim();
									const val = raw.startsWith('#') ? raw : '#' + raw;
									if (/^#[0-9a-fA-F]{6}$/.test(val)) {
										draft = { ...draft, [field.key]: val.toLowerCase() };
									}
								}}
								placeholder="#000000"
								maxlength={7}
								class="w-full px-2 py-1 text-xs font-mono text-center rounded-md border border-slate-200 dark:border-slate-600
									bg-slate-50 dark:bg-slate-700/50 text-slate-700 dark:text-slate-300
									focus:outline-none focus:ring-2 focus:ring-slate-300 dark:focus:ring-slate-600 focus:border-transparent"
							/>
						</label>
					{/each}
				</div>

				<div class="flex items-center justify-between pt-1 border-t border-slate-100 dark:border-slate-700/50">
					<div class="flex items-center gap-2">
						{#each [draft.bg, draft.surface, draft.accent] as color}
							<span
								class="block w-8 h-8 rounded-lg border border-black/10 dark:border-white/10 shrink-0"
								style="background:{color}"
							></span>
						{/each}
						<span class="text-xs text-slate-400 dark:text-slate-500">
							{customIsDark ? 'Dark' : 'Light'} theme
						</span>
					</div>

					<button
						onclick={applyCustomTheme}
						class="px-3.5 py-2 rounded-lg text-sm font-medium bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 hover:bg-slate-700 dark:hover:bg-white transition-colors"
					>
						{customActive ? 'Update' : 'Apply'}
					</button>
				</div>

				{#if customActive}
					<p class="text-xs text-green-600 dark:text-green-400 flex items-center gap-1">
						<Icon name="check" class="w-3 h-3" />
						Custom theme is active
					</p>
				{/if}
			</div>
		</div>
	</section>
</div>
