<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { user } from '$lib/user.svelte';
	import UserBar from '$lib/components/UserBar.svelte';

	$effect(() => {
		if (!user.current) goto(resolve('/login'));
	});
</script>

<svelte:head><title>Profile · Klatsch</title></svelte:head>

{#if user.current}
	<UserBar />
	<div class="profile">
		<h1>Profile</h1>
	</div>
{:else}
	<p>Redirecting to login…</p>
{/if}

<style>
	.profile {
		max-width: 400px;
		margin: 2rem auto;
		padding: 1rem;
	}
</style>
