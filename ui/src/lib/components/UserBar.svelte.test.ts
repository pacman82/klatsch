import { render } from 'vitest-browser-svelte';
import { expect, test, vi, beforeEach } from 'vitest';
import UserBar from './UserBar.svelte';
import { user } from '$lib/user.svelte';
import { user_cache } from '$lib/user_cache.svelte';

const ALICE_ID = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';

beforeEach(() => {
	user.login(ALICE_ID);
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
	vi.spyOn(user_cache, 'resolve').mockReturnValue({ name: 'Alice' });

	const screen = render(UserBar);

	await expect.element(screen.getByText('Logged in as Alice')).toBeVisible();
});

test('displays fetching user info while name is loading', async () => {
	vi.spyOn(user_cache, 'resolve').mockReturnValue(undefined);

	const screen = render(UserBar);

	await expect.element(screen.getByText('Fetching user info...')).toBeVisible();
});

test('calls logout endpoint then clears local session', async () => {
	const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
	vi.stubGlobal('fetch', fetchMock);
	vi.spyOn(user_cache, 'resolve').mockReturnValue({ name: 'Alice' });

	const screen = render(UserBar);
	await screen.getByRole('button', { name: 'Log out' }).click();

	expect(fetchMock).toHaveBeenCalledWith('/api/v0/logout', { method: 'POST' });
	await vi.waitFor(() => expect(user.current).toBeNull());
});

test('logs out when the current user is unknown to the server', async () => {
	vi.spyOn(user_cache, 'resolve').mockReturnValue(null);

	render(UserBar);

	await vi.waitFor(() => expect(user.current).toBeNull());
});
