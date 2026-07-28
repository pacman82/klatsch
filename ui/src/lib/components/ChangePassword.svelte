<script lang="ts">
	type ChangePasswordResponse =
		{ kind: 'success' } | { kind: 'wrong_password' } | { kind: 'server_error' };

	let current_password = $state('');
	let new_password = $state('');
	let confirm_password = $state('');

	// New password and confirm password must match. This helps prevent typos.
	const passwords_match = $derived(new_password === confirm_password);

	let change_password_result = $state<ChangePasswordResponse | null>(null);

	async function change_password(e: MouseEvent) {
		e.preventDefault();
		change_password_result = null;
		const response = await fetch('/api/v0/change_password', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ current_password, new_password })
		});
		if (!response.ok) {
			change_password_result =
				response.status === 401 ? { kind: 'wrong_password' } : { kind: 'server_error' };
			return;
		}
		current_password = '';
		new_password = '';
		confirm_password = '';
		change_password_result = { kind: 'success' };
	}
</script>

<form class="change-password">
	<h2>Change password</h2>
	<input
		type="password"
		class="input"
		bind:value={current_password}
		placeholder="Current password"
		autocomplete="off"
	/>
	<input
		type="password"
		class="input"
		bind:value={new_password}
		placeholder="New password"
		autocomplete="off"
	/>
	<input
		type="password"
		class="input"
		bind:value={confirm_password}
		placeholder="Confirm new password"
		autocomplete="off"
	/>
	{#if !passwords_match}
		<p class="error">New password and confirmation do not match</p>
	{/if}
	<button type="submit" class="btn-primary" onclick={change_password} disabled={!passwords_match}
		>Change password</button
	>
	{#if change_password_result}
		{#if change_password_result.kind === 'success'}
			<p class="change-password-success">Password changed</p>
		{:else if change_password_result.kind === 'wrong_password'}
			<p class="error">Current password is wrong</p>
		{:else if change_password_result.kind === 'server_error'}
			<p class="error">Something went wrong, please try again</p>
		{/if}
	{/if}
</form>

<style>
	.change-password {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}
	h2 {
		margin: 0;
		font-size: 1.25rem;
	}
	.change-password-success {
		font-size: 0.875rem;
		text-align: center;
		margin: 0;
	}
	button:hover:not(:disabled) {
		background: var(--color-primary-hover);
	}
	button:disabled {
		background: #a5b4fc; /* lighter indigo */
		color: #eef2ff; /* soft white */
		cursor: not-allowed;
		opacity: 0.7;
	}
</style>
