import { expect, test, vi, beforeEach } from 'vitest';
import { redirect_to_signup_if_no_users } from './bootstrap';

const goto_mock = vi.hoisted(() => vi.fn());
vi.mock('$app/navigation', () => ({ goto: goto_mock }));

beforeEach(() => {
	goto_mock.mockClear();
});

test('redirects to signup when the system has no users', async () => {
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify(true), { status: 200 }))
	);

	await redirect_to_signup_if_no_users();

	expect(goto_mock).toHaveBeenCalledWith('/signup');
});

test('does not redirect when users already exist', async () => {
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify(false), { status: 200 }))
	);

	await redirect_to_signup_if_no_users();

	expect(goto_mock).not.toHaveBeenCalled();
});
