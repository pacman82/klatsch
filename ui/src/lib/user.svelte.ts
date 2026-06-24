import { browser } from '$app/environment';

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
		if (browser) localStorage.removeItem('user');
	}
};
