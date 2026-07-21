<script lang="ts">
	import '@fontsource/inter/400.css';
	import '@fontsource/inter/600.css';
	import '@fontsource/inter/700.css';
	import '@fontsource/noto-sans-thai/400.css';
	import '@fontsource/noto-sans-thai/600.css';
	import '@fontsource/noto-sans-thai/700.css';
	import '../app.css';
	import { browser } from '$app/environment';
	import { settings } from '$lib/i18n.svelte';

	let { children } = $props();

	// Reflect the reactive locale/theme onto <html> (and the theme-color meta)
	// on mount and on every toggle. The pre-paint script in app.html sets the
	// same attributes first, so there is no flash.
	$effect(() => {
		if (!browser) return;
		document.documentElement.lang = settings.locale;
		document.documentElement.dataset.theme = settings.theme;
		const meta = document.querySelector('meta[name="theme-color"]');
		if (meta) meta.setAttribute('content', settings.theme === 'dark' ? '#0f141a' : '#F8F8FF');
	});
</script>

{@render children()}
