<script lang="ts">
	import { onMount } from 'svelte';
	import { user } from '$lib/user.svelte';

	let name = $state<string | null>(null);

	onMount(async () => {
		while (name === null) {
			try {
				const response = await fetch(`/api/v0/users/${user.current_id}`);
				const data = await response.json();
				name = data.name;
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
	<span>Logged in as <strong>{name}</strong></span>
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
