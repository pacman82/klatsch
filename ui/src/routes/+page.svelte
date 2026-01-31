<script lang="ts">
	import ChatMessages from '$lib/components/ChatMessages.svelte';
	import SendMessage from '$lib/components/SendMessage.svelte';
	import UserPicker from '$lib/components/UserPicker.svelte';
</script>

<svelte:head><title>Klatsch</title></svelte:head>

<UserPicker />
<ChatMessages />
<SendMessage />
