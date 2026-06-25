<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import Icon, { type IconName } from '$lib/components/Icon.svelte';
	import { mode, initTheme, cycleMode, modeIcon, modeLabel } from '$lib/stores/theme';

	let { children } = $props();

	onMount(() => initTheme());

	// ── Sidebar collapse (persisted in localStorage for instant restore) ──────
	const SIDEBAR_KEY = 'read-flow-sidebar-collapsed';
	let collapsed = $state(browser ? localStorage.getItem(SIDEBAR_KEY) === 'true' : false);

	function toggleSidebar(): void {
		collapsed = !collapsed;
		localStorage.setItem(SIDEBAR_KEY, String(collapsed));
	}

	// ── Navigation ────────────────────────────────────────────────────────────
	function isActive(href: string): boolean {
		if (href === '/') return $page.url.pathname === '/';
		return $page.url.pathname.startsWith(href);
	}

	const navLinks: { href: string; label: string; icon: IconName }[] = [
		{ href: '/', label: 'Dashboard', icon: 'home' },
		{ href: '/library', label: 'Library', icon: 'library' },
		{ href: '/online-library', label: 'Online library', icon: 'globe' },
		{ href: '/settings', label: 'Settings', icon: 'settings' },
	];

	// Derived values — updated reactively when $mode changes
	const currentIcon = $derived(modeIcon($mode) as IconName);
	const currentLabel = $derived(modeLabel($mode));
</script>

<div class="h-dvh flex flex-col md:flex-row overflow-hidden bg-slate-50 dark:bg-slate-900">

	<!-- ── Desktop sidebar ────────────────────────────────────── -->
	<!--
		overflow-hidden clips text that hasn't yet disappeared during the width
		transition, preventing wrapping or horizontal overflow mid-animation.
	-->
	<aside
		class="hidden md:flex flex-col shrink-0 border-r border-slate-200 dark:border-slate-700
			bg-white dark:bg-slate-800 overflow-hidden transition-[width] duration-200
			{collapsed ? 'w-14' : 'w-56'}"
	>
		<!-- Logo / title -->
		<div
			class="flex items-center border-b border-slate-100 dark:border-slate-700/50 px-3 py-4
				{collapsed ? 'justify-center' : 'gap-2.5 px-5'}"
		>
			<Icon name="library" class="w-5 h-5 text-slate-700 dark:text-slate-300 shrink-0" />
			{#if !collapsed}
				<span class="font-semibold tracking-tight whitespace-nowrap">
					Read Flow
				</span>
			{/if}
		</div>

		<!-- Navigation links -->
		<nav class="flex-1 py-3 px-2 space-y-0.5 overflow-y-auto">
			<a
				href="/"
				title="Dashboard"
				class="flex items-center px-3 py-2 rounded-md text-sm transition-colors
					{collapsed ? 'justify-center' : 'gap-3'}
					{isActive('/')
						? 'bg-accent/10 text-accent font-medium'
						: 'text-slate-600 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-black dark:hover:text-white'}"
			>
				<Icon name="home" class="w-4 h-4 shrink-0" />
				{#if !collapsed}<span class="whitespace-nowrap">Dashboard</span>{/if}
			</a>

			<a
				href="/library"
				title="Library"
				class="flex items-center px-3 py-2 rounded-md text-sm transition-colors
					{collapsed ? 'justify-center' : 'gap-3'}
					{isActive('/library')
						? 'bg-accent/10 text-accent font-medium'
						: 'text-slate-600 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-black dark:hover:text-white'}"
			>
				<Icon name="library" class="w-4 h-4 shrink-0" />
				{#if !collapsed}<span class="whitespace-nowrap">Library</span>{/if}
			</a>

			<a
				href="/online-library"
				title="Online library"
				class="flex items-center px-3 py-2 rounded-md text-sm transition-colors
					{collapsed ? 'justify-center' : 'gap-3'}
					{isActive('/online-library')
						? 'bg-accent/10 text-accent font-medium'
						: 'text-slate-600 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-black dark:hover:text-white'}"
			>
				<Icon name="globe" class="w-4 h-4 shrink-0" />
				{#if !collapsed}<span class="whitespace-nowrap">Online library</span>{/if}
			</a>

			<a
				href="/settings"
				title="Settings"
				class="flex items-center px-3 py-2 rounded-md text-sm transition-colors
					{collapsed ? 'justify-center' : 'gap-3'}
					{$page.url.pathname === '/settings'
						? 'bg-accent/10 text-accent font-medium'
						: 'text-slate-600 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-black dark:hover:text-white'}"
			>
				<Icon name="settings" class="w-4 h-4 shrink-0" />
				{#if !collapsed}<span class="whitespace-nowrap">Settings</span>{/if}
			</a>

			<!-- Settings sub-pages — only shown when expanded and on a settings page -->
			{#if !collapsed && isActive('/settings')}
				<div class="ml-4 pl-3 border-l border-slate-200 dark:border-slate-600 space-y-0.5">
					<a
						href="/settings/sources"
						class="flex items-center gap-2.5 px-3 py-1.5 rounded-md text-sm transition-colors
							{isActive('/settings/sources')
								? 'bg-accent/10 text-accent font-medium'
								: 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-slate-800 dark:hover:text-slate-200'}"
					>
						<Icon name="server" class="w-3.5 h-3.5 shrink-0" />
						<span class="whitespace-nowrap">Sources</span>
					</a>
					<a
						href="/settings/admin"
						class="flex items-center gap-2.5 px-3 py-1.5 rounded-md text-sm transition-colors
							{isActive('/settings/admin')
								? 'bg-accent/10 text-accent font-medium'
								: 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-slate-800 dark:hover:text-slate-200'}"
					>
						<Icon name="settings" class="w-3.5 h-3.5 shrink-0" />
						<span class="whitespace-nowrap">Server admin</span>
					</a>
					<a
						href="/settings/theme"
						class="flex items-center gap-2.5 px-3 py-1.5 rounded-md text-sm transition-colors
							{isActive('/settings/theme')
								? 'bg-accent/10 text-accent font-medium'
								: 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-slate-800 dark:hover:text-slate-200'}"
					>
						<Icon name="sun" class="w-3.5 h-3.5 shrink-0" />
						<span class="whitespace-nowrap">Theme</span>
					</a>
				</div>
			{/if}
		</nav>

		<!-- Footer: theme toggle + collapse button -->
		<div class="px-2 py-3 border-t border-slate-100 dark:border-slate-700/50 space-y-0.5">
			<!-- Theme -->
			<button
				onclick={() => cycleMode($mode)}
				title={currentLabel}
				class="w-full flex items-center px-3 py-2 rounded-md text-sm transition-colors
					{collapsed ? 'justify-center' : 'gap-3'}
					text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-black dark:hover:text-white"
			>
				<Icon name={currentIcon} class="w-4 h-4 shrink-0" />
				{#if !collapsed}<span class="whitespace-nowrap">{currentLabel}</span>{/if}
			</button>

			<!-- Collapse / expand toggle -->
			<button
				onclick={toggleSidebar}
				title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
				aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
				class="w-full flex items-center px-3 py-2 rounded-md text-sm transition-colors
					{collapsed ? 'justify-center' : 'gap-3'}
					text-slate-400 dark:text-slate-500 hover:bg-slate-50 dark:hover:bg-slate-700/50 hover:text-slate-700 dark:hover:text-slate-300"
			>
				<!--
					-rotate-90 on chevron-down → points right (expand)
					 rotate-90 on chevron-down → points left  (collapse)
				-->
				<Icon
					name="chevron-down"
					class="w-4 h-4 shrink-0 transition-transform duration-200
						{collapsed ? '-rotate-90' : 'rotate-90'}"
				/>
				{#if !collapsed}<span class="whitespace-nowrap text-xs">Collapse</span>{/if}
			</button>
		</div>
	</aside>

	<!-- ── Mobile top bar ─────────────────────────────────────── -->
	<header
		class="md:hidden flex items-center justify-between px-4 h-14 shrink-0 border-b border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800"
	>
		<div class="flex items-center gap-2">
			<Icon name="library" class="w-5 h-5 text-slate-700 dark:text-slate-300" />
			<span class="font-semibold tracking-tight">Read Flow</span>
		</div>
		<button
			onclick={() => cycleMode($mode)}
			class="p-2 rounded-lg text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
			aria-label="Switch theme"
		>
			<Icon name={currentIcon} class="w-5 h-5" />
		</button>
	</header>

	<!-- ── Main content ────────────────────────────────────────── -->
	<main
		class="flex-1 overflow-y-auto md:pb-0"
		style="padding-bottom: calc(4rem + env(safe-area-inset-bottom, 0px))"
	>
		<div class="max-w-screen-2xl mx-auto w-full h-full">
			{@render children()}
		</div>
	</main>

	<!-- ── Mobile bottom navigation ───────────────────────────── -->
	<nav
		class="md:hidden fixed bottom-0 left-0 right-0 bg-white dark:bg-slate-800 border-t border-slate-200 dark:border-slate-700 flex items-start"
		style="padding-bottom: env(safe-area-inset-bottom, 0px)"
	>
		{#each navLinks as link}
			<a
				href={link.href}
				class="flex-1 flex flex-col items-center gap-1 pt-2 pb-2 text-xs font-medium transition-colors min-h-[44px]
					{isActive(link.href) ? 'text-accent' : 'text-slate-400 dark:text-slate-500 hover:text-slate-700 dark:hover:text-slate-300'}"
			>
				<Icon name={link.icon} class="w-5 h-5" />
				<span>{link.label}</span>
			</a>
		{/each}
	</nav>
</div>
