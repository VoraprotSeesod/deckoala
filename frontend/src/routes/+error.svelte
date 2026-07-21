<script lang="ts">
	import { page } from '$app/state';
</script>

<svelte:head>
	<title>Error — Deckoala</title>
</svelte:head>

<main>
	<img src="/logo.svg" alt="" width="96" height="96" />
	<h1>Something went wrong</h1>
	<p class="detail">{page.status}{page.error?.message ? ` — ${page.error.message}` : ''}</p>
	<nav>
		<a href="/app">Try again</a>
		<a href="/login">Go to sign-in</a>
		<a href="/">Home</a>
	</nav>
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

	h1 {
		font-size: 1.6rem;
		margin: 0;
	}

	.detail {
		opacity: 0.7;
		margin: 0;
	}

	nav {
		display: flex;
		gap: 1.25rem;
		margin-top: 0.75rem;
	}

	a {
		color: inherit;
		font-weight: 600;
	}
</style>
