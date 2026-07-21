import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// Native dev: Vite serves the UI, the Rust backend runs on 8080 (cargo run).
		// changeOrigin MUST stay false: the backend's CSRF origin-check compares
		// the browser's Origin against Host, so the proxied Host must remain
		// localhost:5173 (see BRIEF-0001).
		proxy: {
			'/api': { target: 'http://127.0.0.1:8080', changeOrigin: false }
		}
	}
});
