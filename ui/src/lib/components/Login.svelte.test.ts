import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import Login from './Login.svelte';
import { user } from '$lib/user.svelte';

test('submitting an empty name does not call the server', async () => {
	const fetchSpy = vi.fn();
	vi.stubGlobal('fetch', fetchSpy);

	const screen = render(Login);
	await screen.getByRole('button', { name: 'Join' }).click();

	expect(fetchSpy).not.toHaveBeenCalled();
});

test('server error is displayed and button offers retry', async () => {
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(null, { status: 500, statusText: 'test error' }))
	);

	const screen = render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Join' }).click();

	await expect.element(screen.getByText('500 test Error')).toBeVisible();
	await expect.element(screen.getByRole('button', { name: 'Retry' })).toBeVisible();
});

test('password is included in the login request', async () => {
	const ALICE_ID = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
	const fetchSpy = vi
		.fn()
		.mockResolvedValue(new Response(JSON.stringify(ALICE_ID), { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password').fill('secret');
	await screen.getByRole('button', { name: 'Join' }).click();

	expect(fetchSpy).toHaveBeenCalledWith(
		'/api/v0/users',
		expect.objectContaining({ body: JSON.stringify({ name: 'Alice', password: 'secret' }) })
	);
});

test('login stores the user id returned by the server', async () => {
	const id = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify(id), { status: 200 }))
	);

	const screen = render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Join' }).click();

	expect(user.current).toBe(id);
});
