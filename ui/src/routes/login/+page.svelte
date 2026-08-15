<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { user } from '$lib/user.svelte';
	import Login from '$lib/components/Login.svelte';

	$effect(() => {
		if (user.current) goto(resolve('/'));
	});
</script>

<svelte:head><title>Log in · Klatsch</title></svelte:head>

{#if !user.current}
	<div class="login-page">
		<h1>Klatsch</h1>
		<Login />
	</div>
{/if}

<style>
	.login-page {
		max-width: 400px;
		margin: 20vh auto;
		padding: 2rem;
		text-align: center;
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	h1 {
		margin: 0;
		font-size: 2rem;
	}
</style>
