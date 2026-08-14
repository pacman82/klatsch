<script lang="ts">
	import SignUp from '$lib/components/SignUp.svelte';
</script>

<svelte:head><title>Sign up · Klatsch</title></svelte:head>

<div class="signup-page">
	<h1>Sign up</h1>
	<SignUp />
</div>

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
