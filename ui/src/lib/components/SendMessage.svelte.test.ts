import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import SendMessage from './SendMessage.svelte';

test('resubmitting after failure reuses the same message id', async () => {
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
