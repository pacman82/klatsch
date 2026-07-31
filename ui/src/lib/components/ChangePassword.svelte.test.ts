import { render } from 'vitest-browser-svelte';
import { expect, test, vi, beforeEach } from 'vitest';
import ChangePassword from './ChangePassword.svelte';

const user_double = vi.hoisted(() => ({ logout: vi.fn() }));
vi.mock('$lib/user.svelte', () => ({ user: user_double }));

beforeEach(() => {
	user_double.logout.mockClear();
});

async function fill_form(
	screen: Awaited<ReturnType<typeof render>>,
	current: string,
	next: string,
	confirm: string
) {
	await screen.getByPlaceholder('Current password').fill(current);
	await screen.getByPlaceholder('New password', { exact: true }).fill(next);
	await screen.getByPlaceholder('Confirm new password').fill(confirm);
}

test('mismatched new password and confirmation shows an error', async () => {
	const fetchSpy = vi.fn();
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(ChangePassword);
	await fill_form(screen, 'old-secret', 'new-secret', 'something-else');

	await expect
		.element(screen.getByText('New password and confirmation do not match'))
		.toBeVisible();
	expect(fetchSpy).not.toHaveBeenCalled();
});

test('wrong current password shows a user-friendly message', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 400 })));

	const screen = await render(ChangePassword);
	await fill_form(screen, 'dummy-wrong-secret', 'new-secret', 'new-secret');
	await screen.getByRole('button').click();

	await expect.element(screen.getByText('Current password is wrong')).toBeVisible();
});

test('expired session logs out', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 401 })));

	const screen = await render(ChangePassword);
	await fill_form(screen, 'old-secret', 'new-secret', 'new-secret');
	await screen.getByRole('button').click();

	await vi.waitFor(() => expect(user_double.logout).toHaveBeenCalled());
});

test('server error during change shows a generic message', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 500 })));

	const screen = await render(ChangePassword);
	await fill_form(screen, 'old-secret', 'new-secret', 'new-secret');
	await screen.getByRole('button', { name: 'Change password' }).click();

	await expect.element(screen.getByText('Something went wrong, please try again')).toBeVisible();
});

test('change password button sends current and new password to /api/v0/users/me/change_password', async () => {
	const fetchSpy = vi.fn().mockResolvedValue(new Response(null, { status: 200 }));
	vi.stubGlobal('fetch', fetchSpy);

	const screen = await render(ChangePassword);
	await fill_form(screen, 'old-secret', 'new-secret', 'new-secret');
	await screen.getByRole('button', { name: 'Change password' }).click();

	expect(fetchSpy).toHaveBeenCalledWith(
		'/api/v0/users/me/change_password',
		expect.objectContaining({
			body: JSON.stringify({ current_password: 'old-secret', new_password: 'new-secret' })
		})
	);
});

test('successful change clears the fields and shows a success message', async () => {
	vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 200 })));

	const screen = await render(ChangePassword);
	await fill_form(screen, 'old-secret', 'new-secret', 'new-secret');
	await screen.getByRole('button', { name: 'Change password' }).click();

	await expect.element(screen.getByText('Password changed')).toBeVisible();
	await expect.element(screen.getByPlaceholder('Current password')).toHaveValue('');
	await expect.element(screen.getByPlaceholder('New password', { exact: true })).toHaveValue('');
	await expect.element(screen.getByPlaceholder('Confirm new password')).toHaveValue('');
});
