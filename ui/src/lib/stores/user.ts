import { writable } from 'svelte/store';
import { browser } from '$app/environment';

const stored = browser ? localStorage.getItem('user') : null;
export const user = writable<string | null>(stored);

if (browser) {
	user.subscribe((value) => {
		if (value) {
			localStorage.setItem('user', value);
		} else {
			localStorage.removeItem('user');
		}
	});
}
