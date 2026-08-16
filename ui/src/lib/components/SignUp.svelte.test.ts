import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import SignUp from './SignUp.svelte';
import { user } from '$lib/user.svelte';

test('submit button is disabled while the name is empty', async () => {
	const screen = await render(SignUp);

	await expect.element(screen.getByRole('button', { name: 'Sign up' })).toBeDisabled();
});

test('server error during signup', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 500 })));

	const screen = await render(SignUp);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Sign up' }).click();

	await expect.element(screen.getByText('Something went wrong, please try again')).toBeVisible();
});

test('Creating an already existing user shows a user friendly error', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 400 })));

	const screen = await render(SignUp);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password', { exact: true }).fill('secret');
	await screen.getByPlaceholder('Confirm password').fill('secret');
	await screen.getByRole('button', { name: 'Sign up' }).click();

	await expect.element(screen.getByText('Username is already taken')).toBeVisible();
});

test('mismatched password and confirmation shows an error and blocks submission', async () => {
	const fetchSpy = vi.fn();
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(SignUp);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password', { exact: true }).fill('secret');
	await screen.getByPlaceholder('Confirm password').fill('something-else');

	await expect.element(screen.getByText('Password and confirmation do not match')).toBeVisible();
	expect(fetchSpy).not.toHaveBeenCalled();
});

test('sign up button sends credentials to /api/v0/signup', async () => {
	const fetchSpy = vi
		.fn()
		.mockResolvedValue(new Response(JSON.stringify('dummy'), { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(SignUp);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByPlaceholder('Password', { exact: true }).fill('secret');
	await screen.getByPlaceholder('Confirm password').fill('secret');
	await screen.getByRole('button', { name: 'Sign up' }).click();

	expect(fetchSpy).toHaveBeenCalledWith(
		'/api/v0/signup',
		expect.objectContaining({ body: JSON.stringify({ name: 'Alice', password: 'secret' }) })
	);
});

test('signup stores the user id returned by the server', async () => {
	const id = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify(id), { status: 200 }))
	);

	const screen = await render(SignUp);
	await screen.getByPlaceholder('Your name').fill('Alice');
	await screen.getByRole('button', { name: 'Sign up' }).click();

	expect(user.current).toBe(id);
});
