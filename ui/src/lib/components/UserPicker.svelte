<script lang="ts">
	import { onMount } from 'svelte';
	import { user } from '$lib/stores/user';

	let name = '';

	onMount(() => {
		// initialize input with current store value
		name = $user;
	});

	function updateUser() {
		const trimmed = name.trim();
		if (trimmed) user.set(trimmed);
	}
</script>

<div class="user-picker">
	<label for="username">Me</label>
	<input id="username" bind:value={name} on:blur={updateUser} maxlength="32" />
	<button type="button" on:click={updateUser}>Save</button>
</div>

<style>
	.user-picker {
		position: fixed;
		top: 1rem;
		right: 2rem;
		z-index: 100;
		max-width: 600px;
		display: flex;
		gap: 0.5rem;
		align-items: center;
		background: rgba(255, 255, 255, 0.95);
		border-radius: 8px;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.07);
		padding: 0.4rem 0.7rem;
		margin: 0;
	}
	input {
		flex: 1;
		padding: 0.35rem;
		border-radius: 6px;
		border: 1px solid #ccc;
	}
	button {
		padding: 0.35rem 0.6rem;
		border-radius: 6px;
		background: #6366f1;
		color: white;
		border: none;
	}
</style>
