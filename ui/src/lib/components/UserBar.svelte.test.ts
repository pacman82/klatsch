import { render } from 'vitest-browser-svelte';
import { expect, test, vi, beforeEach } from 'vitest';
import UserBar from './UserBar.svelte';
import { user } from '$lib/user.svelte';

const ALICE_ID = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';

beforeEach(() => {
	user.login('Alice', ALICE_ID);
});

test('retries fetching the user name every 5 seconds after a failure', async () => {
	vi.useFakeTimers();
	const fetchStub = vi
		.fn()
		.mockRejectedValueOnce(new Error('Test error'))
		.mockResolvedValueOnce(new Response(JSON.stringify({ name: 'Alice' }), { status: 200 }));
	vi.stubGlobal('fetch', fetchStub);

	const screen = render(UserBar);

	await vi.advanceTimersByTimeAsync(5000);

	await expect.element(screen.getByText('Logged in as Alice')).toBeVisible();
	vi.useRealTimers();
});

test('displays the user name', async () => {
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify({ name: 'Alice' }), { status: 200 }))
	);

	const screen = render(UserBar);

	await expect.element(screen.getByText('Logged in as Alice')).toBeVisible();
});

test('displays fetching user info, if initial fetch fails', async () => {
	vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Test error')));

	const screen = render(UserBar);

	await expect.element(screen.getByText('Fetching user info...')).toBeVisible();
});
