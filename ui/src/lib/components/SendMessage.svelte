<script lang="ts">
	import { v7 } from 'uuid';
	import { user } from '$lib/stores/user';

	type SendMessage = {
		id: string;
		sender: string;
		content: string;
	};

	let message_content = $state('');
	// We are keepin track of the last message that we tried (and failed) to send. This allows us to
	// remember the generated id for the message and rely on the idempotency of the API for retrying
	// the sending.
	//
	// The actual retry will be triggered by the user. He/she can resubmit the same message without
	// fear of creating duplicates. Resetting the pending message to null between submits allows for
	// equal (not same) message to be sent in succession.
	let pending: { id: string; content: string } | null = $state(null);

	// Reuses the id if the content is unchanged (retry), generates a new one otherwise.
	function messageToSend(content: string): SendMessage {
		if (pending?.content !== content) {
			pending = { id: v7(), content };
		}
		return { id: pending.id, sender: $user, content };
	}

	async function handleSubmit(e: SubmitEvent) {
		// We do not want the page to be reloaded, if we submit the message. Therfore we call
		// preventDefault which to my understanding would submit the page as a from and trigger a reload
		// of the entire page.
		e.preventDefault();
		const content = message_content.trim();
		if (!content) return;

		try {
			const response = await fetch('/api/v0/add_message', {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json'
				},
				body: JSON.stringify(messageToSend(content))
			});

			if (!response.ok) {
				console.error('Failed to send message: ', response.statusText);
				return;
			}

			pending = null;
			message_content = '';
		} catch (error) {
			console.error('Error sending message:', error);
		}
	}
</script>

<form onsubmit={handleSubmit} class="send-message-form">
	<input
		type="text"
		bind:value={message_content}
		placeholder="Type your message..."
		autocomplete="off"
	/>
	<button type="submit">Send</button>
</form>

<style>
	.send-message-form {
		display: flex;
		gap: 0.5rem;
		margin: 1rem auto 0 auto;
		max-width: 600px;
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
