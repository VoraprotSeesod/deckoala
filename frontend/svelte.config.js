import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		// Pure SPA: every route falls back to index.html, served by the Rust backend.
		adapter: adapter({ fallback: 'index.html' })
	}
};

export default config;
