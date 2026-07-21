<svelte:head>
	<title>Deckoala — Markdown presentations, self-hosted</title>
	<meta name="description" content="Write Markdown, get slides. Self-hosted presentations with live preview, PDF export and sharing." />
</svelte:head>

<main>
	<img src="/logo.svg" alt="Deckoala logo" width="160" height="160" />
	<h1>Deckoala</h1>
	<p class="tagline">Markdown presentations, self-hosted.</p>
	<a class="cta" href="/app">Open app</a>
	<p class="hint">This instance is up and healthy. The editor arrives in the next build phase.</p>
</main>

<style>
	main {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: 2rem;
		text-align: center;
	}

	img {
		max-width: 40vw;
		height: auto;
	}

	h1 {
		font-size: clamp(2.5rem, 6vw, 4rem);
		margin: 0;
		letter-spacing: -0.02em;
	}

	.tagline {
		font-size: clamp(1.1rem, 2.5vw, 1.4rem);
		margin: 0;
	}

	.cta {
		display: inline-block;
		margin-top: 0.5rem;
		padding: 0.65rem 1.5rem;
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		font-weight: 600;
		text-decoration: none;
	}

	.hint {
		opacity: 0.6;
		font-size: 0.95rem;
		margin-top: 1rem;
	}
</style>
