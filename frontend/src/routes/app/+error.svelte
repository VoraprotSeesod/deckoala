<script lang="ts">
	import { page } from '$app/state';
	import { t } from '$lib/i18n.svelte';

	// Without this boundary a 404 under /app resolves to the ROOT +error.svelte,
	// which unmounts /app/+layout — taking the header, the command palette and
	// the global key listener with it (BRIEF-0009b review finding 3).
</script>

<svelte:head>
	<title>{t('error.title')} — Deckoala</title>
</svelte:head>

<section>
	<h1>{page.status === 404 ? t('error.notFound') : t('error.title')}</h1>
	{#if page.error?.message && page.status !== 404}
		<p class="detail">{page.error.message}</p>
	{/if}
	<a class="button" href="/app">{t('fonts.decksLink')}</a>
</section>

<style>
	section {
		max-width: 32rem;
		margin: 3rem auto;
		text-align: center;
	}

	h1 {
		font-size: 1.4rem;
		margin: 0 0 0.5rem;
	}

	.detail {
		opacity: 0.7;
		font-size: 0.9rem;
	}

	.button {
		display: inline-block;
		margin-top: 1rem;
		font-weight: 600;
		padding: 0.45rem 0.9rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		color: var(--dk-ink);
		text-decoration: none;
	}
</style>
