<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError, type AiProvider } from '$lib/api';
	import { t } from '$lib/i18n.svelte';

	let { data } = $props();
	// svelte-ignore state_referenced_locally
	const isAdmin = data.user.isAdmin;

	// svelte-ignore state_referenced_locally
	let settings = $state(data.settings);

	// --- AI form ---
	// svelte-ignore state_referenced_locally
	let enabled = $state(data.settings?.ai.enabled ?? false);
	// svelte-ignore state_referenced_locally
	let provider = $state<AiProvider>(data.settings?.ai.provider ?? 'anthropic');
	// svelte-ignore state_referenced_locally
	let baseUrl = $state(data.settings?.ai.baseUrl ?? '');
	// svelte-ignore state_referenced_locally
	let model = $state(data.settings?.ai.model ?? '');
	let apiKey = $state(''); // always blank — the server never sends the stored key
	let removeApiKey = $state(false);

	let busy = $state(false);
	let errorMsg = $state('');
	let okMsg = $state('');

	// --- password form ---
	let currentPassword = $state('');
	let newPassword = $state('');
	let pwBusy = $state(false);
	let pwError = $state('');
	let pwOk = $state('');

	async function saveAi(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		errorMsg = '';
		okMsg = '';
		try {
			settings = await api.admin.putSettings({
				enabled,
				provider,
				baseUrl: baseUrl.trim(),
				model: model.trim(),
				apiKey: apiKey.trim() || undefined,
				removeApiKey
			});
			apiKey = '';
			removeApiKey = false;
			enabled = settings.ai.enabled;
			okMsg = t('admin.saved');
			// The /app layout load caches `user.aiEnabled`; without this the
			// editor's AI button stays hidden (or stale) until a hard refresh.
			await invalidateAll();
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : t('admin.saveFailed');
		} finally {
			busy = false;
		}
	}

	async function changePassword(event: SubmitEvent) {
		event.preventDefault();
		pwBusy = true;
		pwError = '';
		pwOk = '';
		try {
			await api.changePassword(currentPassword, newPassword);
			currentPassword = '';
			newPassword = '';
			pwOk = t('admin.passwordChanged');
			if (settings) settings = { ...settings, rootPasswordIsDefault: false };
		} catch (e) {
			pwError = e instanceof ApiError ? e.message : t('admin.passwordFailed');
		} finally {
			pwBusy = false;
		}
	}
</script>

<svelte:head>
	<title>{t('admin.title')} — Deckoala</title>
</svelte:head>

<section>
	<div class="head">
		<h1>{t('admin.title')}</h1>
		<a class="button" href="/app">{t('editor.backDecks')}</a>
	</div>

	{#if !isAdmin || !settings}
		<p class="subtle">{t('admin.onlyAdmin')}</p>
	{:else}
		{#if settings.rootPasswordIsDefault}
			<p class="warn" role="alert">{t('admin.defaultPwWarning')}</p>
		{/if}

		<form onsubmit={changePassword}>
			<h2>{t('admin.password')}</h2>
			{#if pwError}<p class="error" role="alert">{pwError}</p>{/if}
			{#if pwOk}<p class="ok">{pwOk}</p>{/if}
			<div class="row">
				<label>
					{t('admin.currentPassword')}
					<input type="password" bind:value={currentPassword} autocomplete="current-password" required />
				</label>
				<label>
					{t('admin.newPassword')}
					<input
						type="password"
						bind:value={newPassword}
						autocomplete="new-password"
						minlength="8"
						required
					/>
				</label>
			</div>
			<button type="submit" disabled={pwBusy}>{t('admin.changePassword')}</button>
		</form>

		<form onsubmit={saveAi}>
			<h2>{t('admin.ai')}</h2>
			<p class="subtle">{t('admin.aiHint')}</p>
			{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}
			{#if okMsg}<p class="ok">{okMsg}</p>{/if}

			<label class="check">
				<input type="checkbox" bind:checked={enabled} />
				{t('admin.aiEnabled')}
			</label>

			<label>
				{t('admin.provider')}
				<select bind:value={provider}>
					<option value="anthropic">{t('admin.providerAnthropic')}</option>
					<option value="openai">{t('admin.providerOpenai')}</option>
				</select>
			</label>

			<div class="row">
				<label>
					{t('admin.baseUrl')}
					<input bind:value={baseUrl} placeholder="https://api.anthropic.com" />
				</label>
				<label>
					{t('admin.model')}
					<input bind:value={model} placeholder="claude-sonnet-4-6" />
				</label>
			</div>

			<label>
				{t('admin.apiKey')}
				<input type="password" bind:value={apiKey} autocomplete="off" placeholder="••••••••" />
			</label>
			<p class="subtle">
				{#if settings.ai.apiKeySet}
					{t('admin.apiKeyStored', { last4: settings.ai.apiKeyLast4 ?? '' })}
				{:else}
					{t('admin.apiKeyNone')}
				{/if}
			</p>
			{#if settings.ai.apiKeySet}
				<label class="check">
					<input type="checkbox" bind:checked={removeApiKey} />
					{t('admin.removeKey')}
				</label>
			{/if}

			<button type="submit" disabled={busy}>{t('admin.save')}</button>
		</form>
	{/if}
</section>

<style>
	section {
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

	h2 {
		font-size: 1.05rem;
		margin: 0 0 0.5rem;
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

	.subtle {
		opacity: 0.7;
		font-size: 0.9rem;
		margin: 0.25rem 0;
	}

	.warn {
		padding: 0.6rem 0.8rem;
		border: 1.5px solid var(--dk-warn);
		border-radius: 0.5rem;
		background: var(--dk-warn-bg);
		color: var(--dk-warn-ink);
		font-weight: 600;
	}

	.error {
		color: var(--dk-danger);
	}

	.ok {
		color: var(--dk-success);
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		margin-top: 1.25rem;
		padding: 1rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: var(--dk-surface);
	}

	.row {
		display: flex;
		gap: 0.6rem;
		flex-wrap: wrap;
	}

	.row label {
		flex: 1;
		min-width: 12rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		font-weight: 600;
	}

	label.check {
		flex-direction: row;
		align-items: center;
		gap: 0.45rem;
	}

	input,
	select {
		font: inherit;
		font-weight: 400;
		padding: 0.45rem 0.55rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.5rem;
		background: var(--dk-surface);
		color: var(--dk-ink);
	}

	label.check input {
		width: auto;
	}

	button {
		font: inherit;
		font-weight: 600;
		padding: 0.5rem 0.9rem;
		border: none;
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		cursor: pointer;
		align-self: flex-start;
	}

	button:disabled {
		opacity: 0.6;
	}
</style>
