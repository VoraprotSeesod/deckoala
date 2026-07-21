<script lang="ts">
	let { data } = $props();
</script>

<svelte:head>
	<title>Dashboard — Deckoala</title>
</svelte:head>

<section>
	<h1>Welcome, {data.user.username}</h1>
	<p>Your decks arrive in the next build phase.</p>
</section>

<style>
	section {
		max-width: 60rem;
		margin: 0 auto;
	}

	h1 {
		font-size: 1.6rem;
		margin: 0 0 0.5rem;
	}

	p {
		opacity: 0.7;
		margin: 0;
	}
</style>
