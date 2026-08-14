<script lang="ts">
	import { user } from '$lib/user.svelte';

	type SignUpError = { kind: 'wrong_credentials' } | { kind: 'server_error' };

	let name = $state('');
	let password = $state('');
	let confirm_password = $state('');
	// Password and confirmation must match. This is to prevent typos.
	const passwords_match = $derived(password === confirm_password);
	const can_submit = $derived(name.trim() !== '' && passwords_match);
	// An error in case the last signup attempt failed. Used to display an error message to the user.
	let signup_error = $state<SignUpError | null>(null);

	async function sign_up(e: MouseEvent) {
		e.preventDefault();
		const trimmed = name.trim();
		if (!trimmed) return;
		signup_error = null;
		const response = await fetch('/api/v0/signup', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ name: trimmed, password })
		});
		if (!response.ok) {
			signup_error =
				response.status === 401 ? { kind: 'wrong_credentials' } : { kind: 'server_error' };
			return;
		}
		const id: string = await response.json();
		user.login(id);
	}
</script>

<form class="signup">
	<label for="name">Choose a name to create your account</label>
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
		autocomplete="new-password"
	/>
	<input
		type="password"
		class="input"
		bind:value={confirm_password}
		placeholder="Confirm password"
		autocomplete="new-password"
	/>
	{#if !passwords_match}
		<p class="error">Password and confirmation do not match</p>
	{/if}
	<button type="submit" class="btn-primary" onclick={sign_up} disabled={!can_submit}
		>Sign up</button
	>
	{#if signup_error}
		<p class="error">
			{#if signup_error.kind === 'wrong_credentials'}
				User name or password is wrong
			{:else}
				Something went wrong, please try again
			{/if}
		</p>
	{/if}
</form>

<style>
	.signup {
		display: flex;
		flex-direction: column;
		gap: 1rem;
	}
	label {
		color: #666;
	}
</style>
