<script lang="ts">
	import { user } from '$lib/stores/user';
</script>

<div class="user-bar">
	<span>Logged in as <strong>{$user}</strong></span>
	<button onclick={() => user.set(null)}>Log out</button>
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
