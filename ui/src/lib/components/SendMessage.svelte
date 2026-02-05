<script lang="ts">
	import { v7 } from 'uuid';
	import { user } from '$lib/stores/user';

	type SendMessage = {
		id: string;
		sender: string;
		content: string;
	};

	let message_content = $state('');

	async function handleSubmit(e: SubmitEvent) {
	    // We do not want the page to be reloaded, if we submit the message. Therfore we call
		// preventDefault which to my understanding would submit the page as a from and trigger a reload
		// of the entire page.
		e.preventDefault();
		if (!message_content.trim()) return;

		let message: SendMessage = {
			id: v7(),
			sender: $user,
			content: message_content.trim()
		};

		try {
			const response = await fetch('/api/v0/add_message', {
				method: 'POST',
				headers: {
					'Content-Type': 'application/json'
				},
				body: JSON.stringify(message)
			});

			if (!response.ok) {
				console.error('Failed to send message: ', response.statusText);
				return;
			}

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
