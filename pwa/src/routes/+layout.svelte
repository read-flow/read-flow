<script lang="ts">
	import '../app.css';
	import { page } from '$app/stores';
	import Icon, { type IconName } from '$lib/components/Icon.svelte';

	let { children } = $props();

	function isActive(href: string): boolean {
		if (href === '/') return $page.url.pathname === '/';
		return $page.url.pathname.startsWith(href);
	}

	const navLinks: { href: string; label: string; icon: IconName }[] = [
		{ href: '/', label: 'Library', icon: 'library' },
		{ href: '/settings', label: 'Settings', icon: 'settings' },
	];
</script>

<div class="h-dvh flex flex-col md:flex-row overflow-hidden bg-slate-50">
	<!-- ── Desktop sidebar ────────────────────────────────────── -->
	<aside class="hidden md:flex flex-col w-56 shrink-0 border-r border-slate-200 bg-white">
		<div class="px-5 py-4 flex items-center gap-2.5 border-b border-slate-100">
			<Icon name="library" class="w-5 h-5 text-slate-700" />
			<span class="font-semibold text-slate-900 tracking-tight">Read Flow</span>
		</div>

		<nav class="flex-1 py-3 px-2 space-y-0.5 overflow-y-auto">
			<a
				href="/"
				class="flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors
					{isActive('/') ? 'bg-slate-100 text-slate-900 font-medium' : 'text-slate-600 hover:bg-slate-50 hover:text-slate-900'}"
			>
				<Icon name="library" class="w-4 h-4 shrink-0" />
				Library
			</a>

			<a
				href="/settings"
				class="flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors
					{isActive('/settings') && !isActive('/settings/sources') ? 'bg-slate-100 text-slate-900 font-medium' : 'text-slate-600 hover:bg-slate-50 hover:text-slate-900'}"
			>
				<Icon name="settings" class="w-4 h-4 shrink-0" />
				Settings
			</a>

			{#if isActive('/settings')}
				<div class="ml-4 pl-3 border-l border-slate-200 space-y-0.5">
					<a
						href="/settings/sources"
						class="flex items-center gap-2.5 px-3 py-1.5 rounded-md text-sm transition-colors
							{isActive('/settings/sources') ? 'bg-slate-100 text-slate-900 font-medium' : 'text-slate-500 hover:bg-slate-50 hover:text-slate-800'}"
					>
						<Icon name="server" class="w-3.5 h-3.5 shrink-0" />
						Sources
					</a>
				</div>
			{/if}
		</nav>
	</aside>

	<!-- ── Mobile top bar ─────────────────────────────────────── -->
	<header
		class="md:hidden flex items-center justify-between px-4 h-14 shrink-0 border-b border-slate-200 bg-white"
	>
		<div class="flex items-center gap-2">
			<Icon name="library" class="w-5 h-5 text-slate-700" />
			<span class="font-semibold text-slate-900 tracking-tight">Read Flow</span>
		</div>
	</header>

	<!-- ── Main content ────────────────────────────────────────── -->
	<!--
		Bottom padding accounts for the fixed bottom nav bar (h-16 = 4rem) plus the
		iOS home-indicator safe area, so content is never obscured on mobile.
		On md+ there is no bottom nav, so no extra padding is needed.
	-->
	<main
		class="flex-1 overflow-y-auto md:pb-0"
		style="padding-bottom: calc(4rem + env(safe-area-inset-bottom, 0px))"
	>
		{@render children()}
	</main>

	<!-- ── Mobile bottom navigation ───────────────────────────── -->
	<nav
		class="md:hidden fixed bottom-0 left-0 right-0 bg-white border-t border-slate-200 flex items-start"
		style="padding-bottom: env(safe-area-inset-bottom, 0px)"
	>
		{#each navLinks as link}
			<a
				href={link.href}
				class="flex-1 flex flex-col items-center gap-1 pt-2 pb-2 text-xs font-medium transition-colors min-h-[44px]
					{isActive(link.href) ? 'text-slate-900' : 'text-slate-400 hover:text-slate-700'}"
			>
				<Icon name={link.icon} class="w-5 h-5" />
				<span>{link.label}</span>
			</a>
		{/each}
	</nav>
</div>
