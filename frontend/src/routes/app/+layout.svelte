<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import SettingsToggle from '$lib/components/SettingsToggle.svelte';

	let { data, children } = $props();
	let logoutError = $state('');

	async function logout() {
		logoutError = '';
		try {
			await api.logout();
			goto('/login');
		} catch {
			// Do NOT navigate: the session is still alive — pretending we
			// signed out would be worse than admitting the failure.
			logoutError = t('nav.logoutError');
		}
	}
</script>

<div class="shell">
	<header>
		<a class="brand" href="/app">
			<img src="/logo.svg" alt="" width="28" height="28" />
			<span>Deckoala</span>
		</a>
		<div class="session">
			{#if logoutError}<span class="logout-error" role="alert">{logoutError}</span>{/if}
			<a class="navlink" href="/app/fonts">{t('nav.fonts')}</a>
			<a class="navlink" href="/app/tokens">{t('nav.tokens')}</a>
			{#if data.user.isAdmin}
				<a class="navlink" href="/app/admin">{t('nav.admin')}</a>
			{/if}
			<span class="user">{data.user.username}</span>
			<SettingsToggle />
			<button onclick={logout}>{t('nav.logout')}</button>
		</div>
	</header>
	<main>
		{@render children()}
	</main>
</div>

<style>
	.shell {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	.brand {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-weight: 700;
		text-decoration: none;
		color: inherit;
	}

	.session {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		min-width: 0;
	}

	.user {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.navlink {
		font-weight: 600;
		font-size: 0.9rem;
		color: inherit;
		text-decoration: none;
	}

	.navlink:hover {
		text-decoration: underline;
	}

	.logout-error {
		color: var(--dk-danger);
		font-size: 0.85rem;
	}

	button {
		font: inherit;
		font-weight: 600;
		padding: 0.4rem 0.8rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	main {
		flex: 1;
		padding: 1.5rem 1rem;
	}
</style>
