import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import Login from './Login.svelte';
import { user } from '$lib/user.svelte';

test('submitting an empty name does not call the server', async () => {
	const fetchSpy = vi.fn();
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(Login);
	await screen.getByRole('button', { name: 'Log in' }).click();

	expect(fetchSpy).not.toHaveBeenCalled();
});

test('server error during authentication', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 500 })));

	const screen = await render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Log in' }).click();

	await expect.element(screen.getByText('Something went wrong, please try again')).toBeVisible();
});

test('wrong credentials show a user-friendly message', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 401 })));

	const screen = await render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password').fill('wrong');
	await screen.getByRole('button', { name: 'Log in' }).click();

	await expect.element(screen.getByText('User name or password is wrong')).toBeVisible();
});

test('log in button sends credentials to /api/v0/login', async () => {
	const fetchSpy = vi
		.fn()
		.mockResolvedValue(new Response(JSON.stringify('dummy'), { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password').fill('secret');
	await screen.getByRole('button', { name: 'Log in' }).click();

	expect(fetchSpy).toHaveBeenCalledWith(
		'/api/v0/login',
		expect.objectContaining({ body: JSON.stringify({ name: 'Alice', password: 'secret' }) })
	);
});

test('sign up button sends credentials to /api/v0/signup', async () => {
	const fetchSpy = vi
		.fn()
		.mockResolvedValue(new Response(JSON.stringify('dummy'), { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password').fill('secret');
	await screen.getByRole('button', { name: 'Sign up' }).click();

	expect(fetchSpy).toHaveBeenCalledWith(
		'/api/v0/signup',
		expect.objectContaining({ body: JSON.stringify({ name: 'Alice', password: 'secret' }) })
	);
});

test('login stores the user id returned by the server', async () => {
	const id = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify(id), { status: 200 }))
	);

	const screen = await render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Log in' }).click();

	expect(user.current).toBe(id);
});

test('signup stores the user id returned by the server', async () => {
	const id = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify(id), { status: 200 }))
	);

	const screen = await render(Login);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Sign up' }).click();

	expect(user.current).toBe(id);
});
