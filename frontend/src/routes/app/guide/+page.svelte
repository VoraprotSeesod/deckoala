<script lang="ts">
	import GuideBody from '$lib/components/GuideBody.svelte';
	import { t } from '$lib/i18n.svelte';
</script>

<svelte:head>
	<title>{t('guide.title')} — Deckoala</title>
</svelte:head>

<section class="page">
	<div class="head">
		<h1>{t('guide.title')}</h1>
		<a class="button" href="/app">{t('fonts.decksLink')}</a>
	</div>
	<p class="lede">{t('guide.lede')}</p>
	<!-- Standalone manual: Copy only (no editor cursor to insert into here). -->
	<GuideBody />
</section>

<style>
	.page {
		max-width: 48rem;
		margin: 0 auto;
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
	}

	h1 {
		font-size: 1.6rem;
		margin: 0;
	}

	.lede {
		opacity: 0.75;
		font-size: 0.95rem;
		margin: 0.25rem 0 1.5rem;
	}

	.button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.4rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		color: var(--dk-ink);
		text-decoration: none;
	}
</style>
