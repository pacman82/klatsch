<script lang="ts">
	let message = '';

	function handleSubmit(_event: Event) {
		// Dummy implementation: just log the message
		console.log('Sending message:', message);
		message = '';
	}
</script>

<!-- We do not want the page to be reloaded, if we submit the message. Therfore we specify
preventDefault on the submit handler.-->
<form on:submit|preventDefault={handleSubmit} class="send-message-form">
	<input type="text" bind:value={message} placeholder="Type your message..." autocomplete="off" />
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
