<script lang="ts">
	import { onMount } from 'svelte';
	import { user } from '$lib/stores/user';

	type ChatMessage = {
		id: string;
		sender: string;
		content: string;
		// Unix timestamp, milliseconds since epoch UTC
		timestamp_ms: number;
	};

	let messages: ChatMessage[] = $state([]);
	let disconnected = $state(false);
	let serverError: string | null = $state(null);

	onMount(() => {
		const eventSource = new EventSource('/api/v0/events');
		eventSource.onmessage = (event) => {
			const msg: ChatMessage = JSON.parse(event.data);
			messages = [...messages, msg];
		};
		eventSource.onopen = () => {
			disconnected = false;
			serverError = null;
		};
		eventSource.onerror = (e) => {
			if (e instanceof MessageEvent) {
				serverError = e.data;
			}
			disconnected = true;
		};
		return () => {
			eventSource.close();
		};
	});
</script>

<div class="chat-container">
	{#each messages as msg (msg.id)}
		<div class="message-row {msg.sender == $user ? 'me' : 'them'}">
			<div class="message-content">
				<div class="bubble">
					{#if !(msg.sender == $user)}
						<span class="sender">{msg.sender}</span>
					{/if}
					<span class="bubble-content">{msg.content}</span>
				</div>
				<div class="meta">{new Date(msg.timestamp_ms).toString()}</div>
			</div>
		</div>
	{/each}
	{#if disconnected}
		<p class="receive-error">
			{#if serverError}
				Server error: "{serverError}". Reconnecting...
			{:else}
				Error connecting to server. Reconnecting...
			{/if}
		</p>
	{/if}
</div>

<style>
	.chat-container {
		max-width: 600px;
		margin: 2rem auto;
		padding: 1rem;
		background: #f5f7fa;
		border-radius: 12px;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.04);
		min-height: 300px;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.message-row {
		display: flex;
		justify-content: flex-start;
	}

	.message-row.me {
		justify-content: flex-end;
	}

	.message-content {
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		align-items: flex-start;
		max-width: 70%;
	}

	.message-row.me .message-content {
		align-items: flex-end;
	}

	.bubble {
		max-width: 100%;
		padding: 0.5rem 1rem 0.5rem 1rem;
		border-radius: 18px;
		background: #e5e7eb;
		color: #222;
		position: relative;
		word-break: break-word;
		font-size: 1rem;
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
		transition: background 0.2s;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.15rem;
	}

	.message-row.me .bubble {
		background: #6366f1;
		color: #fff;
		border-bottom-right-radius: 8px;
		border-bottom-left-radius: 18px;
		border-top-right-radius: 18px;
		border-top-left-radius: 18px;
		align-items: flex-end;
	}

	.message-row.them .bubble {
		background: #f3f4f6;
		color: #222;
		border-bottom-left-radius: 8px;
		border-bottom-right-radius: 18px;
		border-top-right-radius: 18px;
		border-top-left-radius: 18px;
		align-items: flex-start;
	}

	.sender {
		font-weight: 600;
		color: #6366f1;
		font-size: 0.97rem;
		text-align: left;
		width: 100%;
		display: block;
	}

	.bubble-content {
		width: 100%;
		text-align: left;
		word-break: break-word;
	}

	.message-row.me .bubble-content {
		text-align: right;
	}

	.meta {
		font-size: 0.78rem;
		color: #b0b0b0;
		padding: 0 0.2rem;
		user-select: none;
		text-align: left;
		width: 100%;
	}

	.message-row.me .meta {
		text-align: right;
	}

	.receive-error {
		color: #dc2626;
		font-size: 0.875rem;
		text-align: center;
		margin: 0;
	}
</style>
