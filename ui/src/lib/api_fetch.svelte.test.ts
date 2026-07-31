import { expect, test, vi, beforeEach } from 'vitest';
import { api_fetch } from './api_fetch';

const user_double = vi.hoisted(() => ({ logout: vi.fn() }));
vi.mock('./user.svelte', () => ({ user: user_double }));

beforeEach(() => {
	user_double.logout.mockClear();
});

test('logs out on a 401 response', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 401 })));

	await api_fetch('/api/v0/add_message');

	expect(user_double.logout).toHaveBeenCalled();
});

test('does not log out on a successful response', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })));

	await api_fetch('/api/v0/add_message');

	expect(user_double.logout).not.toHaveBeenCalled();
});

test('returns the response unchanged', async () => {
	const response = new Response('body', { status: 200 });
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response));

	const result = await api_fetch('/api/v0/add_message');

	expect(result).toBe(response);
});

test('forwards the request init to fetch', async () => {
	const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
	vi.stubGlobal('fetch', fetchMock);

	await api_fetch('/api/v0/add_message', { method: 'POST', body: '{}' });

	expect(fetchMock).toHaveBeenCalledWith('/api/v0/add_message', { method: 'POST', body: '{}' });
});
