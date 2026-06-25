import { expect, test, vi } from 'vitest';
import { create_user_cache } from './user_cache.svelte';

test('cache is initially empty', () => {
	vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error()));
	const cache = create_user_cache();
	expect(cache.resolve('unknown-id')).toBeUndefined();
});

test('cache miss triggers a fetch', () => {
	const fetchSpy = vi.fn().mockRejectedValue(new Error());
	vi.stubGlobal('fetch', fetchSpy);
	const cache = create_user_cache();

	cache.resolve('some-id');

	expect(fetchSpy).toHaveBeenCalledWith('/api/v0/users/some-id');
});

test('fetched user is returned by subsequent resolve', async () => {
	const ID = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify({ name: 'Bob' }), { status: 200 }))
	);
	const cache = create_user_cache();

	cache.resolve(ID);

	await vi.waitFor(() => expect(cache.resolve(ID)).toEqual({ name: 'Bob' }));
});
