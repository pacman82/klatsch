import { browser } from '$app/environment';

let current_id = $state<string | null>(browser ? localStorage.getItem('user_id') : null);

export const user = {
	get current_id() {
		return current_id;
	},
	login(id: string) {
		current_id = id;
		if (browser) localStorage.setItem('user_id', id);
	},
	logout() {
		current_id = null;
		if (browser) localStorage.removeItem('user_id');
	}
};
