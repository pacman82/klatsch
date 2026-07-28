<script lang="ts">
	type ChangePasswordResponse =
		| { kind: 'success' }
		| { kind: 'wrong_password' }
		| { kind: 'server_error' };

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
		bind:value={current_password}
		placeholder="Current password"
		autocomplete="off"
	/>
	<input type="password" bind:value={new_password} placeholder="New password" autocomplete="off" />
	<input
		type="password"
		bind:value={confirm_password}
		placeholder="Confirm new password"
		autocomplete="off"
	/>
	{#if !passwords_match}
		<p class="change-password-error">New password and confirmation do not match</p>
	{/if}
	<button type="submit" onclick={change_password} disabled={!passwords_match}
		>Change password</button
	>
	{#if change_password_result}
		{#if change_password_result.kind === 'success'}
			<p class="change-password-success">Password changed</p>
		{:else if change_password_result.kind === 'wrong_password'}
			<p class="change-password-error">Current password is wrong</p>
		{:else if change_password_result.kind === 'server_error'}
			<p class="change-password-error">Something went wrong, please try again</p>
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
	.change-password-error {
		color: #dc2626;
		font-size: 0.875rem;
		margin: 0;
	}
	.change-password-success {
		color: #16a34a;
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
	button:hover:not(:disabled) {
		background: #4f46e5;
	}
	button:disabled {
		background: #a5b4fc; /* lighter indigo */
		color: #eef2ff; /* soft white */
		cursor: not-allowed;
		opacity: 0.7;
	}
</style>
