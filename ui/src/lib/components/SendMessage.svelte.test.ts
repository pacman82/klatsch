import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import SendMessage from './SendMessage.svelte';

test('resubmitting same text after failure is considered retry of same message', async () => {
	// Given a server that rejects requests
	const fetchSpy = vi.fn().mockResolvedValue(new Response(null, { status: 500 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = render(SendMessage);

	// When the user submits a message
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button', { name: 'Send' }).click();

	// and retries the same message
	await screen.getByRole('button', { name: 'Send' }).click();

	// Then both requests carried the same message id
	expect(fetchSpy).toHaveBeenCalledTimes(2);
	const first = JSON.parse(fetchSpy.mock.calls[0][1].body);
	const second = JSON.parse(fetchSpy.mock.calls[1][1].body);
	expect(first.id).toBe(second.id);
});

test('same text send twice is still considered different messages', async () => {
	// Given a server that accepts requests
	const fetchSpy = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = render(SendMessage);

	// When the user sends a message successfully
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button', { name: 'Send' }).click();

	// and sends the same content again
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button', { name: 'Send' }).click();

	// Then the two requests carried different message ids
	expect(fetchSpy).toHaveBeenCalledTimes(2);
	const first = JSON.parse(fetchSpy.mock.calls[0][1].body);
	const second = JSON.parse(fetchSpy.mock.calls[1][1].body);
	expect(first.id).not.toBe(second.id);
});
