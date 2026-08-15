<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { user } from '$lib/user.svelte';
	import SignUp from '$lib/components/SignUp.svelte';

	$effect(() => {
		if (user.current) goto(resolve('/'));
	});
</script>

<svelte:head><title>Sign up · Klatsch</title></svelte:head>

{#if !user.current}
	<div class="signup-page">
		<h1>Sign up</h1>
		<SignUp />
	</div>
{/if}

<style>
	.signup-page {
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
	}
</style>
