import { api_fetch } from './api_fetch';

export type User = {
	name: string;
};

export function create_user_cache() {
	const cache = $state<Record<string, User | null>>({});

	// We use `fetching` to deduplicate requests fetching user info. This does not need reactivity.
	// eslint-disable-next-line svelte/prefer-svelte-reactivity
	const fetching = new Set<string>();

	async function fetch_user(id: string) {
		if (id in cache || fetching.has(id)) return;
		fetching.add(id);
		while (!(id in cache)) {
			try {
				const response = await api_fetch(`/api/v0/users/${id}`);
				// Querying removed user ids won't happen. Yet we can start a different instace with different
				// persistence on the same endpoint. Most likely happens during development.
				if (response.status === 404) {
					cache[id] = null;
					break;
				}
				// api_fetch already logged us out and redirected; retrying would just hammer a dead session.
				if (response.status === 401) break;
				if (!response.ok) throw new Error(`${response.status}`);
				const data: { name: string } = await response.json();
				cache[id] = { name: data.name };
			} catch {
				await new Promise((resolve) => setTimeout(resolve, 5000));
			}
		}
		fetching.delete(id);
	}

	return {
		// User info if already cached. If not triggers a fetch and returns undefined immediately.
		// Returns null if the server confirmed the user does not exist (no retry).
		// Svelte reactivity takes care of updating the result once the fetch completes.
		resolve(id: string): User | undefined | null {
			if (!(id in cache)) void fetch_user(id);
			return cache[id];
		}
	};
}

export const user_cache = create_user_cache();
