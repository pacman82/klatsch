<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { api_fetch } from '$lib/api_fetch';
	import { user } from '$lib/user.svelte';
	import { user_cache } from '$lib/user_cache.svelte';

	const show_back_to_chat = $derived(page.url.pathname !== '/');

	const user_info = $derived(user_cache.resolve(user.current!));

	$effect(() => {
		if (user_info === null) user.logout();
	});

	async function logout() {
		await fetch('/api/v0/logout', { method: 'POST' });
		user.logout();
	}

	let copied = $state(false);

	async function copy_invite_link() {
		const response = await api_fetch('/api/v0/invites', { method: 'POST' });
		const token: string = await response.json();
		await navigator.clipboard.writeText(`${location.origin}/invite/${token}`);
	}
</script>

<div class="top-bar">
	{#if show_back_to_chat}
		<a href={resolve('/')} class="back">← Back to chat</a>
	{/if}
	<div class="copy-invite">
		<button
			onclick={async () => {
				await copy_invite_link();
				copied = true;
			}}
			onmouseleave={() => (copied = false)}
			onblur={() => (copied = false)}
		>
			Copy invite link
		</button>
		{#if copied}
			<span class="tooltip">Copied!</span>
		{/if}
	</div>
	<div class="account">
		{#if user_info}
			<span>Logged in as <a href={resolve('/profile')}><strong>{user_info.name}</strong></a></span>
		{:else}
			<span>Fetching user info...</span>
		{/if}
		<button onclick={logout}>Log out</button>
	</div>
</div>

<style>
	.top-bar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 0.75rem;
		padding: 0.5rem 1rem;
		background: rgba(255, 255, 255, 0.95);
		border-bottom: 1px solid #e5e7eb;
	}
	.account {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}
	.back {
		margin-right: auto;
	}
	.copy-invite {
		position: relative;
	}
	.tooltip {
		position: absolute;
		top: calc(100% + 0.3rem);
		left: 50%;
		transform: translateX(-50%);
		background: #333;
		color: #fff;
		font-size: 0.75rem;
		padding: 0.2rem 0.5rem;
		border-radius: 4px;
		white-space: nowrap;
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
	a {
		color: inherit;
		text-decoration: none;
	}
	a:hover {
		text-decoration: underline;
	}
</style>
