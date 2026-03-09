<script lang="ts">
	import { user } from '$lib/stores/user';
	import Login from '$lib/components/Login.svelte';
	import UserBar from '$lib/components/UserBar.svelte';
	import ChatMessages from '$lib/components/ChatMessages.svelte';
	import SendMessage from '$lib/components/SendMessage.svelte';
</script>

<svelte:head><title>Klatsch</title></svelte:head>

{#if $user}
	<UserBar />
	<ChatMessages />
	<SendMessage />
{:else}
	<Login />
{/if}
