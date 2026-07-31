import { user } from './user.svelte';

export async function api_fetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
	const response = await fetch(input, init);
	if (response.status === 401) {
		user.logout();
	}
	return response;
}
