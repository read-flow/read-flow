<script lang="ts">
	// Reader routes use a full-screen layout with no sidebar or bottom nav.
	// The individual reader pages manage their own chrome (back button, toolbar, etc.).
	let { children } = $props();
</script>

<div class="h-dvh w-full overflow-hidden bg-slate-900">
	{@render children()}
</div>
