<script lang="ts">
	import { onMount } from 'svelte';
	import { user } from '$lib/user.svelte';

	type User = {
		name: string;
	};
	let name = $state<string | null>(null);
	let user_info = $state<User | null>(null);

	onMount(async () => {
		while (name === null) {
			try {
				const response = await fetch(`/api/v0/users/${user.current_id}`);
				user_info = await response.json();
			} catch {
				await new Promise((resolve) => setTimeout(resolve, 5000));
			}
		}
	});

	function logout() {
		user.logout();
	}
</script>

<div class="user-bar">
	{#if user_info !== null}
		<span>Logged in as <strong>{user_info.name}</strong></span>
	{:else}
		<span>Fetching user info...</span>
	{/if}
	<button onclick={logout}>Log out</button>
</div>

<style>
	.user-bar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 0.75rem;
		padding: 0.5rem 1rem;
		background: rgba(255, 255, 255, 0.95);
		border-bottom: 1px solid #e5e7eb;
	}
	button {
		padding: 0.3rem 0.6rem;
		border-radius: 6px;
		border: 1px solid #ccc;
		background: white;
		cursor: pointer;
		font-size: 0.875rem;
	}
	button:hover {
		background: #f3f4f6;
	}
</style>
