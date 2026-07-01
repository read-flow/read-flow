<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { sources, loadSources } from '$lib/stores/sources';
	import {
		ReadFlowClient,
		type ScanSummary,
		type CheckMissingResponse,
		type ScanDirectoryEntry,
		type ServerSettingsDto,
		type UserDto,
	} from '$lib/api/client';

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

	// ── Scan directories ───────────────────────────────────────────────────────
	let scanDirs = $state<ScanDirectoryEntry[]>([]);
	let dirsError = $state('');
	let dirsBusy = $state(false);

	// New-entry form
	let newPath = $state('');
	let newAction = $state<'Scan' | 'Ignore'>('Scan');
	let newTags = $state('');
	let newInherit = $state(false);

	$effect(() => {
		// Reload directories whenever the selected server changes.
		const id = selectedId;
		void id;
		scanDirs = [];
		dirsError = '';
		if (!client) return;
		const c = client;
		void (async () => {
			try {
				scanDirs = await c.getScanDirectories();
			} catch (err) {
				dirsError = err instanceof Error ? err.message : 'Failed to load directories.';
			}
		})();
	});

	async function addDir() {
		if (!client || dirsBusy || !newPath.trim()) return;
		dirsBusy = true;
		dirsError = '';
		const entry: ScanDirectoryEntry = {
			path: newPath.trim(),
			action: newAction,
			inherit: newInherit,
			...(newAction === 'Scan'
				? { tags: newTags.split(',').map((t) => t.trim()).filter(Boolean) }
				: {}),
		};
		try {
			scanDirs = await client.putScanDirectory(entry);
			newPath = '';
			newTags = '';
			newInherit = false;
			newAction = 'Scan';
		} catch (err) {
			dirsError = err instanceof Error ? err.message : 'Failed to save directory.';
		} finally {
			dirsBusy = false;
		}
	}

	async function removeDir(path: string) {
		if (!client || dirsBusy) return;
		dirsBusy = true;
		dirsError = '';
		try {
			scanDirs = await client.deleteScanDirectory(path);
		} catch (err) {
			dirsError = err instanceof Error ? err.message : 'Failed to remove directory.';
		} finally {
			dirsBusy = false;
		}
	}

	// ── Server settings ────────────────────────────────────────────────────────
	let settings = $state<ServerSettingsDto | null>(null);
	let extCsv = $state('');
	let tagsCsv = $state('');
	let originsCsv = $state('');
	let maxUploadMiB = $state<number | null>(null);
	let settingsError = $state('');
	let settingsSaving = $state(false);

	const MIB = 1024 * 1024;

	$effect(() => {
		const id = selectedId;
		void id;
		settings = null;
		settingsError = '';
		if (!client) return;
		const c = client;
		void (async () => {
			try {
				const dto = await c.getSettings();
				settings = dto;
				extCsv = dto.extensions.join(', ');
				tagsCsv = dto.private_tags.join(', ');
				originsCsv = dto.allowed_origins.join(', ');
				maxUploadMiB = dto.max_upload_bytes != null ? Math.round(dto.max_upload_bytes / MIB) : null;
			} catch (err) {
				settingsError = err instanceof Error ? err.message : 'Failed to load settings.';
			}
		})();
	});

	function csv(s: string): string[] {
		return s.split(',').map((t) => t.trim()).filter(Boolean);
	}

	async function saveSettings() {
		if (!client || !settings || settingsSaving) return;
		settingsSaving = true;
		settingsError = '';
		const dto: ServerSettingsDto = {
			...settings,
			extensions: csv(extCsv),
			private_tags: csv(tagsCsv),
			allowed_origins: csv(originsCsv),
			max_upload_bytes: maxUploadMiB != null && maxUploadMiB > 0 ? Math.round(maxUploadMiB * MIB) : null,
		};
		try {
			const saved = await client.putSettings(dto);
			settings = saved;
			extCsv = saved.extensions.join(', ');
			tagsCsv = saved.private_tags.join(', ');
			originsCsv = saved.allowed_origins.join(', ');
			maxUploadMiB = saved.max_upload_bytes != null ? Math.round(saved.max_upload_bytes / MIB) : null;
		} catch (err) {
			settingsError = err instanceof Error ? err.message : 'Failed to save settings.';
		} finally {
			settingsSaving = false;
		}
	}

	// ── Authorized users ───────────────────────────────────────────────────────
	let users = $state<UserDto[]>([]);
	let usersError = $state('');
	let usersBusy = $state(false);
	let pwDrafts = $state<Record<string, string>>({});

	// New-user form
	let newUserId = $state('');
	let newUserPw = $state('');
	let newUserOwner = $state(false);

	$effect(() => {
		const id = selectedId;
		void id;
		users = [];
		usersError = '';
		pwDrafts = {};
		if (!client) return;
		const c = client;
		void (async () => {
			try {
				users = await c.getUsers();
			} catch (err) {
				usersError = err instanceof Error ? err.message : 'Failed to load users.';
			}
		})();
	});

	function isOwner(u: UserDto): boolean {
		return u.roles.includes('owner');
	}

	async function addUser() {
		if (!client || usersBusy || !newUserId.trim() || !newUserPw) return;
		usersBusy = true;
		usersError = '';
		try {
			users = await client.createUser(
				newUserId.trim(),
				newUserPw,
				newUserOwner ? ['owner'] : [],
			);
			newUserId = '';
			newUserPw = '';
			newUserOwner = false;
		} catch (err) {
			usersError = err instanceof Error ? err.message : 'Failed to create user.';
		} finally {
			usersBusy = false;
		}
	}

	async function toggleOwner(u: UserDto) {
		if (!client || usersBusy) return;
		usersBusy = true;
		usersError = '';
		try {
			users = await client.updateUser(u.user_id, isOwner(u) ? [] : ['owner']);
		} catch (err) {
			usersError = err instanceof Error ? err.message : 'Failed to update user.';
		} finally {
			usersBusy = false;
		}
	}

	async function resetPassword(u: UserDto) {
		const pw = pwDrafts[u.user_id]?.trim();
		if (!client || usersBusy || !pw) return;
		usersBusy = true;
		usersError = '';
		try {
			users = await client.updateUser(u.user_id, u.roles, pw);
			pwDrafts = { ...pwDrafts, [u.user_id]: '' };
		} catch (err) {
			usersError = err instanceof Error ? err.message : 'Failed to set password.';
		} finally {
			usersBusy = false;
		}
	}

	async function removeUser(u: UserDto) {
		if (!client || usersBusy) return;
		usersBusy = true;
		usersError = '';
		try {
			users = await client.deleteUser(u.user_id);
		} catch (err) {
			usersError = err instanceof Error ? err.message : 'Failed to delete user.';
		} finally {
			usersBusy = false;
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

		<!-- Scan directories -->
		<section class="mb-8">
			<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-2">Scan directories</h2>
			<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50">
				{#if scanDirs.length === 0}
					<p class="px-4 py-3 text-sm text-slate-400 dark:text-slate-500">No directories configured.</p>
				{:else}
					{#each scanDirs as dir}
						<div class="flex items-center gap-3 px-4 py-3">
							<div class="flex-1 min-w-0">
								<p class="text-sm font-mono truncate" title={dir.path}>{dir.path}</p>
								<div class="flex items-center gap-2 mt-0.5">
									<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs font-medium
										{dir.action === 'Scan'
											? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400'
											: 'bg-slate-100 text-slate-600 dark:bg-slate-700 dark:text-slate-400'}">
										{dir.action}
									</span>
									{#if dir.inherit}<span class="text-xs text-slate-400 dark:text-slate-500">inherit</span>{/if}
									{#each dir.tags ?? [] as tag}
										<span class="inline-flex items-center px-1.5 py-0.5 rounded text-xs bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-400">{tag}</span>
									{/each}
								</div>
							</div>
							<button
								onclick={() => removeDir(dir.path)}
								disabled={dirsBusy}
								aria-label="Remove directory"
								class="shrink-0 p-1.5 rounded-lg text-slate-400 dark:text-slate-500 hover:text-red-500 dark:hover:text-red-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
							>
								<Icon name="trash" class="w-4 h-4" />
							</button>
						</div>
					{/each}
				{/if}

				<!-- Add form -->
				<div class="px-4 py-3 space-y-2">
					<input
						type="text"
						bind:value={newPath}
						placeholder="/absolute/path"
						class="w-full px-3 py-2 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-accent/50"
					/>
					<div class="flex flex-wrap items-center gap-2">
						<select
							bind:value={newAction}
							class="rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 px-2 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
						>
							<option value="Scan">Scan</option>
							<option value="Ignore">Ignore</option>
						</select>
						{#if newAction === 'Scan'}
							<input
								type="text"
								bind:value={newTags}
								placeholder="tags (comma-separated)"
								class="flex-1 min-w-[8rem] px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
							/>
						{/if}
						<label class="flex items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400">
							<input type="checkbox" bind:checked={newInherit} class="accent-slate-900 dark:accent-slate-100" />
							inherit
						</label>
						<button
							onclick={addDir}
							disabled={dirsBusy || !newPath.trim()}
							class="ml-auto inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
						>
							<Icon name="plus" class="w-4 h-4" />
							Add
						</button>
					</div>
				</div>
			</div>
			{#if dirsError}<p class="mt-2 text-xs text-red-500 dark:text-red-400">{dirsError}</p>{/if}
		</section>

		<!-- Server settings -->
		<section class="mb-8">
			<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-2">Settings</h2>
			{#if settings}
				<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50 text-sm">
					<div class="flex items-center justify-between px-4 py-3 gap-3">
						<span class="text-slate-500 dark:text-slate-400 shrink-0">Database</span>
						<span class="font-mono text-xs text-slate-400 dark:text-slate-500 truncate" title={settings.database_url}>{settings.database_url}</span>
					</div>
					<div class="flex items-center px-4 py-3 gap-3">
						<label for="ext" class="text-slate-500 dark:text-slate-400 shrink-0">Extensions</label>
						<input id="ext" type="text" bind:value={extCsv} placeholder="pdf, epub"
							class="flex-1 min-w-0 bg-transparent text-right focus:outline-none" />
					</div>
					<div class="flex items-center justify-between px-4 py-3 gap-3">
						<label for="conc" class="text-slate-500 dark:text-slate-400 shrink-0">Concurrency</label>
						<input id="conc" type="number" min="1" bind:value={settings.concurrency}
							class="w-20 bg-transparent text-right focus:outline-none" />
					</div>
					<div class="flex items-center justify-between px-4 py-3">
						<label for="dry" class="text-slate-500 dark:text-slate-400">Dry run</label>
						<input id="dry" type="checkbox" bind:checked={settings.dry_run} class="accent-slate-900 dark:accent-slate-100" />
					</div>
					<div class="flex items-center justify-between px-4 py-3">
						<label for="pm" class="text-slate-500 dark:text-slate-400">Private mode</label>
						<input id="pm" type="checkbox" bind:checked={settings.private_mode} class="accent-slate-900 dark:accent-slate-100" />
					</div>
					<div class="flex items-center px-4 py-3 gap-3">
						<label for="ptags" class="text-slate-500 dark:text-slate-400 shrink-0">Private tags</label>
						<input id="ptags" type="text" bind:value={tagsCsv} placeholder="private"
							class="flex-1 min-w-0 bg-transparent text-right focus:outline-none" />
					</div>
					<div class="flex items-center px-4 py-3 gap-3">
						<label for="origins" class="text-slate-500 dark:text-slate-400 shrink-0">Allowed origins</label>
						<input id="origins" type="text" bind:value={originsCsv} placeholder="any origin"
							class="flex-1 min-w-0 bg-transparent text-right focus:outline-none" />
					</div>
					<div class="flex items-center justify-between px-4 py-3 gap-3">
						<label for="maxup" class="text-slate-500 dark:text-slate-400 shrink-0">Max upload (MiB)</label>
						<input id="maxup" type="number" min="0" bind:value={maxUploadMiB} placeholder="default"
							class="w-24 bg-transparent text-right focus:outline-none" />
					</div>
				</div>
				<p class="mt-2 text-xs text-slate-400 dark:text-slate-500">
					Allowed origins and max upload apply after the server restarts.
				</p>
				<div class="mt-3 flex justify-end">
					<button
						onclick={saveSettings}
						disabled={settingsSaving}
						class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
					>
						{settingsSaving ? 'Saving…' : 'Save settings'}
					</button>
				</div>
			{:else if !settingsError}
				<p class="text-sm text-slate-400 dark:text-slate-500">Loading…</p>
			{/if}
			{#if settingsError}<p class="mt-2 text-xs text-red-500 dark:text-red-400">{settingsError}</p>{/if}
		</section>

		<!-- Authorized users -->
		<section class="mb-8">
			<h2 class="text-xs font-semibold uppercase tracking-wider text-slate-400 dark:text-slate-500 mb-2">Authorized users</h2>
			<div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 divide-y divide-slate-100 dark:divide-slate-700/50">
				{#if users.length === 0}
					<p class="px-4 py-3 text-sm text-slate-400 dark:text-slate-500">No users.</p>
				{:else}
					{#each users as u}
						<div class="px-4 py-3 space-y-2">
							<div class="flex items-center gap-3">
								<div class="flex-1 min-w-0">
									<p class="text-sm font-medium truncate">{u.user_id}</p>
								</div>
								<label class="flex items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400">
									<input
										type="checkbox"
										checked={isOwner(u)}
										disabled={usersBusy}
										onchange={() => toggleOwner(u)}
										class="accent-slate-900 dark:accent-slate-100"
									/>
									owner
								</label>
								<button
									onclick={() => removeUser(u)}
									disabled={usersBusy}
									aria-label="Delete user"
									class="shrink-0 p-1.5 rounded-lg text-slate-400 dark:text-slate-500 hover:text-red-500 dark:hover:text-red-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
								>
									<Icon name="trash" class="w-4 h-4" />
								</button>
							</div>
							<div class="flex items-center gap-2">
								<input
									type="password"
									placeholder="new password"
									value={pwDrafts[u.user_id] ?? ''}
									oninput={(e) => (pwDrafts = { ...pwDrafts, [u.user_id]: (e.currentTarget as HTMLInputElement).value })}
									class="flex-1 min-w-0 px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
								/>
								<button
									onclick={() => resetPassword(u)}
									disabled={usersBusy || !(pwDrafts[u.user_id] ?? '').trim()}
									class="shrink-0 px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 text-sm font-medium hover:bg-slate-50 dark:hover:bg-slate-700 transition-colors disabled:opacity-40"
								>
									Set
								</button>
							</div>
						</div>
					{/each}
				{/if}

				<!-- New user -->
				<div class="px-4 py-3 flex flex-wrap items-center gap-2">
					<input
						type="text"
						bind:value={newUserId}
						placeholder="user id"
						class="flex-1 min-w-[6rem] px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
					/>
					<input
						type="password"
						bind:value={newUserPw}
						placeholder="password"
						class="flex-1 min-w-[6rem] px-3 py-1.5 rounded-lg border border-slate-200 dark:border-slate-600 bg-slate-50 dark:bg-slate-700/50 text-sm focus:outline-none focus:ring-2 focus:ring-accent/50"
					/>
					<label class="flex items-center gap-1.5 text-xs text-slate-500 dark:text-slate-400">
						<input type="checkbox" bind:checked={newUserOwner} class="accent-slate-900 dark:accent-slate-100" />
						owner
					</label>
					<button
						onclick={addUser}
						disabled={usersBusy || !newUserId.trim() || !newUserPw}
						class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 dark:bg-slate-100 text-white dark:text-slate-900 text-sm font-medium hover:bg-slate-700 dark:hover:bg-white transition-colors disabled:opacity-40"
					>
						<Icon name="plus" class="w-4 h-4" />
						Add
					</button>
				</div>
			</div>
			{#if usersError}<p class="mt-2 text-xs text-red-500 dark:text-red-400">{usersError}</p>{/if}
		</section>
	{/if}
</div>
