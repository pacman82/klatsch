<script lang="ts">
	import { user } from '$lib/user.svelte';

	let name = $state('');
	let login_error = $state<string | null>(null);
	let is_retry = $derived(login_error !== null);

	async function join(e: SubmitEvent) {
		e.preventDefault();
		const trimmed = name.trim();
		if (!trimmed) return;
		login_error = null;
		const response = await fetch('/api/v0/users', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: trimmed })
		});
		if (!response.ok) {
			login_error = `${response.status} ${response.statusText}`;
			return;
		}
		const id: string = await response.json();
		user.login(trimmed, id);
	}
</script>

<form onsubmit={join} class="login">
	<h1>Klatsch</h1>
	<label for="name">Enter your name to join</label>
	<div class="login-controls">
		<input id="name" bind:value={name} placeholder="Your name" maxlength="32" autocomplete="off" />
		<button type="submit">{is_retry ? 'Retry' : 'Join'}</button>
	</div>
	{#if login_error}
		<p class="login-error">{login_error}</p>
	{/if}
</form>

<style>
	.login {
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
	label {
		color: #666;
	}
	.login-error {
		color: #dc2626;
		font-size: 0.875rem;
		margin: 0;
	}
	.login-controls {
		display: flex;
		gap: 0.5rem;
	}
	input {
		flex: 1;
		padding: 0.5rem;
		border-radius: 6px;
		border: 1px solid #ccc;
		font-size: 1rem;
	}
	button {
		padding: 0.5rem 1.2rem;
		border-radius: 6px;
		border: none;
		background: #6366f1;
		color: #fff;
		font-weight: bold;
		cursor: pointer;
	}
	button:hover {
		background: #4f46e5;
	}
</style>
