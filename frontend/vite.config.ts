import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		// Native dev: Vite serves the UI, the Rust backend runs on 8080 (cargo run).
		proxy: {
			'/api': 'http://127.0.0.1:8080'
		}
	}
});
