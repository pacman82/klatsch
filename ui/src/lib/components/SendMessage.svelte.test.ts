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
	await screen.getByRole('button').click();

	// and retries the same message
	await screen.getByRole('button').click();

	// Then both requests carried the same message id
	expect(fetchSpy).toHaveBeenCalledTimes(2);
	const first = JSON.parse(fetchSpy.mock.calls[0][1].body);
	const second = JSON.parse(fetchSpy.mock.calls[1][1].body);
	expect(first.id).toBe(second.id);
});

test('server error is displayed to the user', async () => {
	// Given a server that responds with an error
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 500, statusText: 'test error' })));

	const screen = render(SendMessage);

	// When the user submits a message
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button').click();

	// Then the error is displayed and the button offers retry
	await expect.element(screen.getByText('500 test error')).toBeVisible();
	await expect.element(screen.getByRole('button', { name: 'Retry' })).toBeVisible();
});

test('error disappears after successful retry', async () => {
	// Given a message that previously failed to send
	const fetchStub = vi.fn()
		.mockResolvedValueOnce(new Response(null, { status: 500, statusText: 'test error' }))
		.mockResolvedValueOnce(new Response(null, { status: 200 }));
	vi.stubGlobal('fetch', fetchStub);

	const screen = render(SendMessage);
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button').click();

	// When the user retries and it succeeds
	await screen.getByRole('button').click();

	// Then the error disappears and the button says Send again
	await expect.element(screen.getByText('500 test error')).not.toBeInTheDocument();
	await expect.element(screen.getByRole('button', { name: 'Send' })).toBeVisible();
});

test('editing message after failure shows send instead of retry', async () => {
	// Given a message that failed to send
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 500, statusText: 'test error' })));

	const screen = render(SendMessage);
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button').click();

	// When the user edits the message
	await screen.getByPlaceholder('Type your message...').fill('Hello, world!');

	// Then the button says Send, not Retry
	await expect.element(screen.getByRole('button', { name: 'Send' })).toBeVisible();
});

test('same text send twice is still considered different messages', async () => {
	// Given a server that accepts requests
	const fetchSpy = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = render(SendMessage);

	// When the user sends a message successfully
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button').click();

	// and sends the same content again
	await screen.getByPlaceholder('Type your message...').fill('Hello');
	await screen.getByRole('button').click();

	// Then the two requests carried different message ids
	expect(fetchSpy).toHaveBeenCalledTimes(2);
	const first = JSON.parse(fetchSpy.mock.calls[0][1].body);
	const second = JSON.parse(fetchSpy.mock.calls[1][1].body);
	expect(first.id).not.toBe(second.id);
});
