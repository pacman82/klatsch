import { browser } from '$app/environment';

let current = $state<string | null>(browser ? localStorage.getItem('user') : null);
let current_id = $state<string | null>(browser ? localStorage.getItem('user_id') : null);

export const user = {
	get current() {
		return current;
	},
	get current_id() {
		return current_id;
	},
	login(name: string, id: string) {
		current = name;
		current_id = id;
		if (browser) localStorage.setItem('user', name);
		if (browser) localStorage.setItem('user_id', id);
	},
	logout() {
		current = null;
		current_id = null;
		if (browser) localStorage.removeItem('user');
		if (browser) localStorage.removeItem('user_id');
	}
};
