<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, ApiError, type Instance } from '$lib/api';
	import { t } from '$lib/i18n.svelte';
	import SettingsToggle from '$lib/components/SettingsToggle.svelte';

	type Mode = 'first-run' | 'login' | 'register';

	let instance = $state<Instance | null>(null);
	let mode = $state<Mode>('login');
	let username = $state('');
	let password = $state('');
	let error = $state('');
	let busy = $state(false);

	onMount(async () => {
		try {
			await api.me();
			goto('/app');
			return;
		} catch {
			// not signed in — show the form
		}
		try {
			instance = await api.instance();
			mode = instance.hasUsers ? 'login' : 'first-run';
		} catch {
			error = t('login.serverUnreachable');
		}
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		error = '';
		busy = true;
		try {
			if (mode === 'login') {
				await api.login(username, password);
			} else {
				await api.register(username, password);
			}
			goto('/app');
		} catch (e) {
			error = e instanceof ApiError ? e.message : t('common.somethingWrong');
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head>
	<title>{t('login.signIn')} — Deckoala</title>
</svelte:head>

<div class="corner"><SettingsToggle /></div>

<main>
	<form class="card" onsubmit={submit}>
		<img src="/logo.svg" alt="" width="72" height="72" />
		<h1>
			{#if mode === 'first-run'}{t('login.firstRunTitle')}
			{:else if mode === 'register'}{t('login.registerTitle')}
			{:else}{t('login.loginTitle')}{/if}
		</h1>
		{#if mode === 'first-run'}
			<p class="note">{t('login.firstRunNote')}</p>
		{/if}

		<label>
			{t('login.username')}
			<input
				name="username"
				bind:value={username}
				autocomplete="username"
				autocapitalize="none"
				spellcheck="false"
				pattern="[A-Za-z0-9_\-]{'{'}3,32{'}'}"
				title={t('login.usernameHint')}
				required
			/>
		</label>
		<label>
			{t('login.password')}
			<input
				name="password"
				type="password"
				bind:value={password}
				autocomplete={mode === 'login' ? 'current-password' : 'new-password'}
				minlength={mode === 'login' ? undefined : 8}
				required
			/>
		</label>

		{#if error}<p class="error" role="alert">{error}</p>{/if}

		<button type="submit" disabled={busy}>
			{#if mode === 'login'}{t('login.signIn')}{:else}{t('login.register')}{/if}
		</button>

		{#if instance?.hasUsers}
			{#if mode === 'login' && instance.allowSignup}
				<p class="swap">
					{t('login.noAccount')} <a href="/login" onclick={(e) => { e.preventDefault(); mode = 'register'; error = ''; }}>{t('login.createOne')}</a>
				</p>
			{:else if mode === 'register'}
				<p class="swap">
					{t('login.haveAccountQ')} <a href="/login" onclick={(e) => { e.preventDefault(); mode = 'login'; error = ''; }}>{t('login.signIn')}</a>
				</p>
			{/if}
		{/if}
	</form>
</main>

<style>
	.corner {
		position: fixed;
		top: 1rem;
		right: 1rem;
	}

	main {
		min-height: 100dvh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 1.5rem;
	}

	.card {
		width: 100%;
		max-width: 24rem;
		display: flex;
		flex-direction: column;
		gap: 1rem;
		text-align: center;
	}

	.card img {
		align-self: center;
	}

	h1 {
		font-size: 1.5rem;
		margin: 0;
	}

	.note {
		margin: 0;
		opacity: 0.7;
		font-size: 0.95rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
		text-align: left;
		font-weight: 600;
		font-size: 0.95rem;
	}

	input {
		font: inherit;
		font-weight: 400;
		padding: 0.6rem 0.75rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.5rem;
		background: var(--dk-surface);
		color: var(--dk-ink);
	}

	input:focus {
		outline: 2px solid var(--dk-ink);
		outline-offset: 1px;
	}

	button {
		font: inherit;
		font-weight: 600;
		padding: 0.65rem 1rem;
		border: none;
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		cursor: pointer;
	}

	button:disabled {
		opacity: 0.6;
		cursor: wait;
	}

	.error {
		margin: 0;
		color: var(--dk-danger);
		font-size: 0.95rem;
	}

	.swap {
		margin: 0;
		font-size: 0.95rem;
	}

	.swap a {
		color: inherit;
	}
</style>
