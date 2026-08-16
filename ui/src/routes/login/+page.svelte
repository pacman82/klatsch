<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { redirect_to_signup_if_no_users } from '$lib/bootstrap';
	import { user } from '$lib/user.svelte';
	import Login from '$lib/components/Login.svelte';

	$effect(() => {
		if (user.current) goto(resolve('/'));
	});

	onMount(redirect_to_signup_if_no_users);
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
