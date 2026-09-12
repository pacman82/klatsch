<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { user } from '$lib/user.svelte';
	import TopBar from '$lib/components/TopBar.svelte';
	import ChatMessages from '$lib/components/ChatMessages.svelte';
	import SendMessage from '$lib/components/SendMessage.svelte';

	$effect(() => {
		if (!user.current) goto(resolve('/login'));
	});
</script>

<svelte:head><title>Klatsch</title></svelte:head>

{#if user.current}
	<div class="chat-page">
		<TopBar />
		<ChatMessages />
		<SendMessage />
	</div>
{:else}
	<p>Redirecting to login…</p>
{/if}

<style>
	.chat-page {
		display: flex;
		flex-direction: column;
		box-sizing: border-box;
		height: 100dvh;
		gap: 2rem;
		padding-bottom: 1rem;
	}
</style>
