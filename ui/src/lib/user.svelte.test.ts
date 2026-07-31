import { expect, test, vi, beforeEach } from 'vitest';
import { user } from './user.svelte';

const goto_mock = vi.hoisted(() => vi.fn());
vi.mock('$app/navigation', () => ({ goto: goto_mock }));

beforeEach(() => {
	goto_mock.mockClear();
});

test('logout clears the current user', () => {
	user.login('ab70b6ca-4139-499f-a66d-15e88f081fb1');

	user.logout();

	expect(user.current).toBeNull();
});

test('logout navigates to the login page', () => {
	user.login('ab70b6ca-4139-499f-a66d-15e88f081fb1');

	user.logout();

	expect(goto_mock).toHaveBeenCalledWith('/login');
});
