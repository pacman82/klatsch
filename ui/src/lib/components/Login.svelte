<script lang="ts">
	import { user } from '$lib/user.svelte';

	type LoginError = { kind: 'wrong_credentials' } | { kind: 'server_error' };

	// Username used for login
	let name = $state('');
	let password = $state('');
	// An error in case the last login attempt failed. Used to display an error message to the user.
	let login_error = $state<LoginError | null>(null);

	async function log_in(e: MouseEvent) {
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

	async function sign_up(e: MouseEvent) {
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
	<label for="name">Enter your name to join</label>
	<input
		id="name"
		class="input"
		bind:value={name}
		placeholder="Your name"
		maxlength="32"
		autocomplete="off"
	/>
	<input
		type="password"
		class="input"
		bind:value={password}
		placeholder="Password"
		autocomplete="off"
	/>
	<button type="submit" class="btn-primary" onclick={log_in}>Log in</button>
	<button type="submit" class="btn-primary" onclick={sign_up}>Sign up</button>
	{#if login_error}
		<p class="error">
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
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	label {
		color: #666;
	}
</style>
