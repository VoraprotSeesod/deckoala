<script lang="ts">
	import { browser } from '$app/environment';
	import { api, ApiError, type ApiToken } from '$lib/api';
	import { formatDate, t } from '$lib/i18n.svelte';

	let { data } = $props();

	// svelte-ignore state_referenced_locally
	let tokens = $state<ApiToken[]>(data.tokens);
	let name = $state('');
	let errorMsg = $state('');
	let busy = $state(false);
	// The plaintext lives only in this component, only until dismissed.
	let fresh = $state<{ name: string; token: string } | null>(null);
	let copied = $state(false);

	const endpoint = $derived(browser ? `${location.origin}/mcp` : '/mcp');
	// While the one-time value is on screen the snippets are genuinely
	// paste-ready; once dismissed they fall back to a placeholder, because the
	// token can never be retrieved again.
	const secret = $derived(fresh?.token ?? 'dko_…');
	const httpSnippet = $derived(
		`claude mcp add --transport http deckoala ${endpoint} \\\n  --header "Authorization: Bearer ${secret}"`
	);
	const configSnippet = $derived(
		JSON.stringify(
			{
				mcpServers: {
					deckoala: {
						command: 'node',
						args: ['/path/to/mcp-stdio-bridge.mjs'],
						env: { DECKOALA_MCP_URL: endpoint, DECKOALA_MCP_TOKEN: secret }
					}
				}
			},
			null,
			2
		)
	);

	function fail(e: unknown) {
		errorMsg = e instanceof ApiError ? e.message : t('common.somethingWrong');
	}

	async function create(event: SubmitEvent) {
		event.preventDefault();
		busy = true;
		errorMsg = '';
		copied = false;
		try {
			const created = await api.tokens.create(name.trim());
			fresh = { name: created.name, token: created.token };
			name = '';
			tokens = await api.tokens.list();
		} catch (e) {
			fail(e);
		} finally {
			busy = false;
		}
	}

	async function copy() {
		if (!fresh) return;
		try {
			await navigator.clipboard.writeText(fresh.token);
			copied = true;
		} catch {
			// Clipboard can be blocked; the value is on screen to select by hand.
			copied = false;
		}
	}

	async function revoke(token: ApiToken) {
		if (!confirm(t('tokens.revokeConfirm', { name: token.name }))) return;
		errorMsg = '';
		try {
			await api.tokens.revoke(token.id);
			tokens = await api.tokens.list();
		} catch (e) {
			fail(e);
		}
	}
</script>

<svelte:head>
	<title>{t('tokens.title')} — Deckoala</title>
</svelte:head>

<section>
	<div class="head">
		<h1>{t('tokens.title')}</h1>
		<a class="button" href="/app">{t('fonts.decksLink')}</a>
	</div>
	<p class="hint">{t('tokens.hint')}</p>

	<p class="endpoint">
		<span class="label">{t('tokens.endpoint')}</span>
		<code>{endpoint}</code>
	</p>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	{#if fresh}
		<div class="fresh" role="status">
			<strong>{fresh.name}</strong>
			<p class="warn">{t('tokens.createdOnce')}</p>
			<div class="row">
				<code class="secret">{fresh.token}</code>
				<button onclick={copy}>{copied ? t('tokens.copied') : t('tokens.copy')}</button>
				<button
					class="ghost"
					onclick={() => {
						fresh = null;
						copied = false;
					}}>{t('tokens.done')}</button
				>
			</div>
			<p class="subtle">{t('tokens.snippetsLive')}</p>
		</div>
	{/if}

	<form onsubmit={create}>
		<h2>{t('tokens.newToken')}</h2>
		<label>
			{t('tokens.name')}
			<input bind:value={name} required placeholder={t('tokens.namePlaceholder')} />
		</label>
		<button type="submit" disabled={busy}>{t('tokens.create')}</button>
	</form>

	<h2>{t('tokens.existing')}</h2>
	{#if tokens.length === 0}
		<p class="subtle">{t('tokens.none')}</p>
	{:else}
		<ul class="tokens">
			{#each tokens as token (token.id)}
				<li class:revoked={token.revokedAt}>
					<div class="meta">
						<span class="name">{token.name}</span>
						<small>
							{t('tokens.created', { when: formatDate(token.createdAt) })} ·
							{token.lastUsedAt
								? t('tokens.lastUsed', { when: formatDate(token.lastUsedAt) })
								: t('tokens.neverUsed')}
						</small>
					</div>
					{#if token.revokedAt}
						<span class="badge">{t('tokens.revoked')}</span>
					{:else}
						<span class="badge active">{t('tokens.active')}</span>
						<button class="x" onclick={() => revoke(token)}>{t('tokens.revoke')}</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}

	<h2>{t('tokens.configHint')}</h2>
	<p class="hint">{t('tokens.clientsHttp')}</p>
	<pre>{httpSnippet}</pre>
	<p class="hint">{t('tokens.clientsStdio')}</p>
	<pre>{configSnippet}</pre>
	<p class="subtle">{t('tokens.toolsNote')}</p>
</section>

<style>
	section {
		max-width: 55rem;
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
		margin: 1.25rem 0 0.5rem;
	}

	.hint,
	.subtle {
		opacity: 0.7;
		font-size: 0.9rem;
	}

	.error {
		color: var(--dk-danger);
	}

	.endpoint {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
		font-size: 0.9rem;
	}

	.endpoint .label {
		font-weight: 600;
	}

	code {
		background: color-mix(in srgb, var(--dk-ink) 8%, transparent);
		padding: 0.1em 0.4em;
		border-radius: 4px;
		word-break: break-all;
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

	.fresh {
		padding: 1rem;
		margin: 1rem 0;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.75rem;
		background: var(--dk-surface);
	}

	.fresh .warn {
		margin: 0.35rem 0 0.6rem;
		font-size: 0.85rem;
		font-weight: 600;
		color: var(--dk-danger);
	}

	.fresh .row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.secret {
		flex: 1 1 20rem;
		font-size: 0.85rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 1rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: var(--dk-surface);
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		font-weight: 600;
	}

	input {
		font: inherit;
		font-weight: 400;
		padding: 0.4rem 0.5rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.5rem;
		background: var(--dk-surface);
		color: inherit;
	}

	button {
		font: inherit;
		font-weight: 600;
		padding: 0.5rem 0.8rem;
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

	button.ghost {
		background: transparent;
		color: var(--dk-ink);
		border: 1.5px solid var(--dk-ink);
	}

	.tokens {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.tokens li {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 1rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.6rem;
		background: var(--dk-surface);
	}

	.tokens li.revoked {
		opacity: 0.55;
	}

	.meta {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}

	.name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.meta small {
		opacity: 0.65;
		font-size: 0.78rem;
	}

	.badge {
		font-size: 0.75rem;
		font-weight: 600;
		padding: 0.15rem 0.5rem;
		border-radius: 999px;
		border: 1px solid color-mix(in srgb, var(--dk-ink) 20%, transparent);
	}

	.badge.active {
		border-color: color-mix(in srgb, var(--dk-ink) 45%, transparent);
	}

	.x {
		background: transparent;
		color: var(--dk-danger);
		border: 1.5px solid currentColor;
		font-size: 0.8rem;
		padding: 0.25rem 0.6rem;
	}

	pre {
		font-size: 0.8rem;
		padding: 0.85rem 1rem;
		border-radius: 0.6rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		background: var(--dk-surface);
		overflow-x: auto;
	}
</style>
