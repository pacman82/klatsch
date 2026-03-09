import { browser } from '$app/environment';

let current = $state<string | null>(browser ? localStorage.getItem('user') : null);

export const user = {
	get current() {
		return current;
	},
	login(name: string) {
		current = name;
		if (browser) localStorage.setItem('user', name);
	},
	logout() {
		current = null;
		if (browser) localStorage.removeItem('user');
	}
};
