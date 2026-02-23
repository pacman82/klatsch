<script lang="ts">
	import { v7 } from 'uuid';
	import { user } from '$lib/stores/user';

	type SendMessage = {
		id: string;
		sender: string;
		content: string;
	};

	let message_content = $state('');
	// Tracks the last message content and id send. Independent of success.
	let last_attempt = $state<{ id: string; content: string } | null>(null);
	// Outcome of sending the last message. Null on success, otherwise a text describing the error.
	let send_error: string | null = $state(null);
	// If we try to send the **same** message again after a failure it is a retry.
	let is_retry = $derived(
		send_error != null && message_content.trim() === last_attempt?.content
	);

	async function handleSubmit(e: SubmitEvent) {
		// We do not want the page to be reloaded, if we submit the message. Therfore we call
		// preventDefault which to my understanding would submit the page as a from and trigger a reload
		// of the entire page.
		e.preventDefault();
		const content = message_content.trim();
		if (!content) return;

		const id = is_retry ? last_attempt!.id : v7();
		last_attempt = { id, content };
		send_error = null;
		try {
			const msg: SendMessage = { id, sender: $user, content };
			const response = await fetch('/api/v0/add_message', {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json'
				},
				body: JSON.stringify(msg)
			});

			if (!response.ok) {
				send_error = `${response.status} ${response.statusText}`;
				return;
			}

			message_content = '';
		} catch (error) {
			send_error = String(error);
		}
	}
</script>

<form onsubmit={handleSubmit} class="send-message-form">
	{#if send_error}
		<p class="send-error">{send_error}</p>
	{/if}
	<div class="send-controls">
		<input
			type="text"
			bind:value={message_content}
			placeholder="Type your message..."
			autocomplete="off"
		/>
		<button type="submit">{is_retry ? 'Retry' : 'Send'}</button>
	</div>
</form>

<style>
	.send-message-form {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		margin: 1rem auto 0 auto;
		max-width: 600px;
	}
	.send-error {
		color: #dc2626;
		font-size: 0.875rem;
		margin: 0;
	}
	.send-controls {
		display: flex;
		gap: 0.5rem;
	}
	input[type='text'] {
		flex: 1;
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
		transition: background 0.2s;
	}
	button:hover {
		background: #4f46e5;
	}
</style>
