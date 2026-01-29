import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import dotenv from 'dotenv';

dotenv.config();

export default defineConfig({
	plugins: [sveltekit()],
	// Dev server options
	server: {
		// During production, the backend and frontend are served by the same process. In order to support
		// hot reloading during development, we want to use `npm run dev` while working on the frontend.
		// To have all the functionality we need we still run the rust backend server.
		proxy: {
			'/api': backend_url_for_dev()
		}
	}
});

// Rust backend URL during development.
function backend_url_for_dev() {
	const backendHost = process.env.HOST || '127.0.0.1';
	const backendPort = process.env.PORT || '3000';
	return `http://${backendHost}:${backendPort}`;
}
