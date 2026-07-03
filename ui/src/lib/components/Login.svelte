<script lang="ts">
	import { user } from '$lib/user.svelte';

	type LoginError = { kind: 'wrong_credentials' } | { kind: 'server_error' };

	// Username used for login
	let name = $state('');
	let password = $state('');
	// An error in case the last login attempt failed. Used to display an error message to the user.
	let login_error = $state<LoginError | null>(null);

	async function log_in(e: SubmitEvent) {
		e.preventDefault();
		const trimmed = name.trim();
		if (!trimmed) return;
		login_error = null;
		const response = await fetch('/api/v0/login', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: trimmed, password })
		});
		if (!response.ok) {
			login_error =
				response.status === 401 ? { kind: 'wrong_credentials' } : { kind: 'server_error' };
			return;
		}
		const id: string = await response.json();
		user.login(id);
	}

	async function sign_up(e: SubmitEvent) {
		e.preventDefault();
		const trimmed = name.trim();
		if (!trimmed) return;
		login_error = null;
		const response = await fetch('/api/v0/signup', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: trimmed, password })
		});
		if (!response.ok) {
			login_error =
				response.status === 401 ? { kind: 'wrong_credentials' } : { kind: 'server_error' };
			return;
		}
		const id: string = await response.json();
		user.login(id);
	}
</script>

<form class="login">
	<h1>Klatsch</h1>
	<label for="name">Enter your name to join</label>
	<input id="name" bind:value={name} placeholder="Your name" maxlength="32" autocomplete="off" />
	<input type="password" bind:value={password} placeholder="Password" autocomplete="off" />
	<button type="submit" onclick={log_in}>Log in</button>
	<button type="submit" onclick={sign_up}>Sign up</button>
	{#if login_error}
		<p class="login-error">
			{#if login_error.kind === 'wrong_credentials'}
				User name or password is wrong
			{:else}
				Something went wrong, please try again
			{/if}
		</p>
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
	input {
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
