<script lang="ts">
	import { v7 } from 'uuid';

	type SendMessage = {
		id: string;
		sender: string;
		content: string;
	};

	let sender = 'Bob';
	let message_content = '';

	async function handleSubmit(_event: Event) {
		if (!message_content.trim()) return;

		let message: SendMessage = {
			id: v7(),
			sender: sender,
			content: message_content.trim()
		};

		try {
			const response = await fetch('/api/v0/add_message_ffo', {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json'
				},
				body: JSON.stringify({ message })
			});

			if (!response.ok) {
				console.error('Failed to send message: {}', response.statusText);
				return;
			}

			message_content = '';
		} catch (error) {
			console.error('Error sending message:', error);
		}
	}
</script>

<!-- We do not want the page to be reloaded, if we submit the message. Therfore we specify
preventDefault on the submit handler.-->
<form on:submit|preventDefault={handleSubmit} class="send-message-form">
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
