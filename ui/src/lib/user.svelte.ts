import { browser } from '$app/environment';
import { goto } from '$app/navigation';
import { resolve } from '$app/paths';

let current = $state<string | null>(browser ? localStorage.getItem('user') : null);

export const user = {
	get current() {
		return current;
	},
	login(id: string) {
		current = id;
		if (browser) localStorage.setItem('user', id);
	},
	logout() {
		current = null;
		if (browser) {
			localStorage.removeItem('user');
			goto(resolve('/login'));
		}
	}
};
