import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

export async function redirect_to_signup_if_no_users() {
	const response = await fetch('/api/v0/users/is_empty');
	const is_empty: boolean = await response.json();
	if (is_empty) goto(resolve('/signup'));
}
