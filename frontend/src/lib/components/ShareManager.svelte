<script lang="ts">
	import { onMount } from 'svelte';
	import { api, ApiError, type ShareLink, type SharePermission } from '$lib/api';

	let { deckId, onClose }: { deckId: string; onClose: () => void } = $props();

	let links = $state<ShareLink[]>([]);
	let loading = $state(true);
	let errorMsg = $state('');
	let busy = $state(false);
	let copiedId = $state<string | null>(null);

	// new-link form
	let permission = $state<SharePermission>('view');
	let expiresLocal = $state(''); // <input type="datetime-local"> value

	async function refresh() {
		try {
			links = await api.shares.list(deckId);
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : 'Could not load share links.';
		} finally {
			loading = false;
		}
	}

	onMount(refresh);

	function absoluteUrl(path: string): string {
		return `${location.origin}${path}`;
	}

	async function create() {
		busy = true;
		errorMsg = '';
		try {
			// A datetime-local value is naive local time; toISOString() gives a
			// canonical UTC `…Z` the backend accepts. Empty → no expiry.
			const expiresAt = expiresLocal ? new Date(expiresLocal).toISOString() : null;
			await api.shares.create(deckId, permission, expiresAt);
			expiresLocal = '';
			await refresh();
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : 'Could not create the link.';
		} finally {
			busy = false;
		}
	}

	async function revoke(link: ShareLink) {
		if (!confirm(`Revoke this ${link.permission} link? Anyone using it loses access.`)) return;
		errorMsg = '';
		try {
			await api.shares.revoke(deckId, link.id);
			await refresh();
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : 'Could not revoke the link.';
		}
	}

	async function copy(link: ShareLink) {
		try {
			await navigator.clipboard.writeText(absoluteUrl(link.url));
			copiedId = link.id;
			setTimeout(() => {
				if (copiedId === link.id) copiedId = null;
			}, 1500);
		} catch {
			errorMsg = 'Could not copy — select the link and copy manually.';
		}
	}

	function fmt(ts: string | null): string {
		return ts ? new Date(ts).toLocaleString() : '';
	}
</script>

<div
	class="overlay"
	role="button"
	tabindex="0"
	aria-label="Close"
	onclick={onClose}
	onkeydown={(e) => {
		if (e.key === 'Escape' || e.key === 'Enter') onClose();
	}}
></div>
<div class="modal" role="dialog" aria-modal="true" aria-label="Share this deck">
	<div class="head">
		<h2>Share this deck</h2>
		<button class="x" onclick={onClose} aria-label="Close">×</button>
	</div>

	<p class="hint">
		Anyone with a link can open it without an account. <strong>View</strong> links present and export;
		<strong>edit</strong> links can also change the deck.
	</p>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	<form onsubmit={(e) => (e.preventDefault(), create())}>
		<label>
			Access
			<select bind:value={permission}>
				<option value="view">View only</option>
				<option value="edit">Can edit</option>
			</select>
		</label>
		<label>
			Expires (optional)
			<input type="datetime-local" bind:value={expiresLocal} />
		</label>
		<button type="submit" disabled={busy}>Create link</button>
	</form>

	<h3>Existing links</h3>
	{#if loading}
		<p class="hint">Loading…</p>
	{:else if links.length === 0}
		<p class="hint">No links yet.</p>
	{:else}
		<ul>
			{#each links as link (link.id)}
				<li class:inactive={link.status !== 'active'}>
					<div class="row1">
						<span class="badge">{link.permission}</span>
						<span class="status" data-status={link.status}>{link.status}</span>
						{#if link.expiresAt}<span class="exp">until {fmt(link.expiresAt)}</span>{/if}
					</div>
					<div class="row2">
						<input class="url" readonly value={absoluteUrl(link.url)} />
						<button onclick={() => copy(link)}>{copiedId === link.id ? 'Copied' : 'Copy'}</button>
						{#if link.status === 'active'}
							<button class="revoke" onclick={() => revoke(link)}>Revoke</button>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(11, 18, 21, 0.4);
		z-index: 40;
	}

	.modal {
		position: fixed;
		z-index: 41;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(34rem, calc(100vw - 2rem));
		max-height: calc(100dvh - 3rem);
		overflow: auto;
		background: var(--dk-bg);
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.9rem;
		padding: 1.25rem;
		box-shadow: 0 10px 40px rgba(11, 18, 21, 0.25);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	h2 {
		font-size: 1.2rem;
		margin: 0;
	}

	h3 {
		font-size: 0.95rem;
		margin: 1.1rem 0 0.5rem;
	}

	.x {
		border: none;
		background: transparent;
		font-size: 1.5rem;
		line-height: 1;
		cursor: pointer;
		color: var(--dk-ink);
	}

	.hint {
		font-size: 0.85rem;
		opacity: 0.7;
		margin: 0.3rem 0;
	}

	.error {
		color: #b3261e;
		font-size: 0.85rem;
	}

	form {
		display: flex;
		gap: 0.6rem;
		align-items: flex-end;
		flex-wrap: wrap;
		margin-top: 0.75rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.8rem;
		font-weight: 600;
	}

	select,
	input {
		font: inherit;
		font-weight: 400;
		padding: 0.4rem 0.5rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.5rem;
		background: #fff;
	}

	form button {
		font: inherit;
		font-weight: 600;
		padding: 0.5rem 0.8rem;
		border: none;
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		cursor: pointer;
	}

	form button:disabled {
		opacity: 0.6;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	li {
		padding: 0.6rem 0.7rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.6rem;
		background: #fff;
	}

	li.inactive {
		opacity: 0.6;
	}

	.row1 {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.8rem;
		margin-bottom: 0.4rem;
	}

	.badge {
		font-weight: 700;
		text-transform: uppercase;
		font-size: 0.7rem;
		padding: 0.1rem 0.4rem;
		border-radius: 0.3rem;
		background: color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	.status[data-status='active'] {
		color: #157347;
		font-weight: 600;
	}

	.status[data-status='revoked'],
	.status[data-status='expired'] {
		color: #b3261e;
		font-weight: 600;
	}

	.exp {
		opacity: 0.6;
	}

	.row2 {
		display: flex;
		gap: 0.4rem;
		align-items: center;
	}

	.url {
		flex: 1;
		min-width: 0;
		font-size: 0.78rem;
		font-family: ui-monospace, Consolas, monospace;
	}

	.row2 button {
		font: inherit;
		font-size: 0.78rem;
		font-weight: 600;
		padding: 0.35rem 0.55rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.45rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
		white-space: nowrap;
	}

	.row2 button.revoke {
		border-color: #b3261e;
		color: #b3261e;
	}
</style>
