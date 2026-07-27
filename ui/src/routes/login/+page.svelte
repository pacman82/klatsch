<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { user } from '$lib/user.svelte';
	import Login from '$lib/components/Login.svelte';

	$effect(() => {
		if (user.current) goto(resolve('/'));
	});
</script>

<svelte:head><title>Log in · Klatsch</title></svelte:head>

{#if !user.current}
	<Login />
{/if}
