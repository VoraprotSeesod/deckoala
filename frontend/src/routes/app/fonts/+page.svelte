<script lang="ts">
	import { api, ApiError, type FontInfo } from '$lib/api';

	let { data } = $props();
	// svelte-ignore state_referenced_locally
	const isAdmin = data.user.isAdmin;

	// svelte-ignore state_referenced_locally
	let fonts = $state<FontInfo[]>(data.fonts);
	let errorMsg = $state('');
	let busy = $state(false);

	// upload form
	let upFamily = $state('');
	let upWeight = $state('400');
	let upStyle = $state('normal');
	let upFile = $state<FileList | null>(null);

	// google form
	let gFamily = $state('');
	let gWeights = $state('400, 700');

	function fail(e: unknown) {
		errorMsg = e instanceof ApiError ? e.message : 'Something went wrong.';
	}

	async function refresh() {
		fonts = await api.fonts.list();
		api.fonts.reloadCss();
	}

	async function submitUpload(event: SubmitEvent) {
		event.preventDefault();
		const file = upFile?.[0];
		if (!file) return;
		busy = true;
		errorMsg = '';
		try {
			await api.fonts.upload(upFamily.trim(), upWeight, upStyle, file);
			upFamily = '';
			upFile = null;
			await refresh();
		} catch (e) {
			fail(e);
		} finally {
			busy = false;
		}
	}

	async function submitGoogle(event: SubmitEvent) {
		event.preventDefault();
		const weights = gWeights
			.split(',')
			.map((w) => w.trim())
			.filter(Boolean);
		busy = true;
		errorMsg = '';
		try {
			await api.fonts.google(gFamily.trim(), weights);
			gFamily = '';
			await refresh();
		} catch (e) {
			fail(e);
		} finally {
			busy = false;
		}
	}

	async function remove(font: FontInfo) {
		if (!confirm(`Remove ${font.family} ${font.weight} ${font.style}?`)) return;
		errorMsg = '';
		try {
			await api.fonts.remove(font.id);
			await refresh();
		} catch (e) {
			fail(e);
		}
	}

	// Group variants by family for display.
	let byFamily = $derived(
		Object.entries(
			fonts.reduce<Record<string, FontInfo[]>>((acc, f) => {
				(acc[f.family] ??= []).push(f);
				return acc;
			}, {})
		).sort((a, b) => a[0].localeCompare(b[0]))
	);
</script>

<svelte:head>
	<title>Fonts — Deckoala</title>
</svelte:head>

<section>
	<div class="head">
		<h1>Fonts</h1>
		<a class="button" href="/app">← Decks</a>
	</div>
	<p class="hint">
		Installed fonts are served from this instance (no external CDN). Use one in a deck via a Marp
		<code>style</code> directive, e.g. <code>style: | section &#123; font-family: 'Sarabun'; &#125;</code>.
	</p>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	{#if isAdmin}
		<div class="forms">
			<form onsubmit={submitUpload}>
				<h2>Upload a font</h2>
				<label>Family <input bind:value={upFamily} required placeholder="My Font" /></label>
				<div class="row">
					<label>Weight
						<select bind:value={upWeight}>
							{#each ['100', '200', '300', '400', '500', '600', '700', '800', '900'] as w (w)}
								<option value={w}>{w}</option>
							{/each}
						</select>
					</label>
					<label>Style
						<select bind:value={upStyle}>
							<option value="normal">normal</option>
							<option value="italic">italic</option>
						</select>
					</label>
				</div>
				<label>File <input type="file" accept=".woff2,.woff,.ttf,.otf" bind:files={upFile} required /></label>
				<button type="submit" disabled={busy}>Upload</button>
			</form>

			<form onsubmit={submitGoogle}>
				<h2>Install from Google Fonts</h2>
				<label>Family <input bind:value={gFamily} required placeholder="Sarabun" /></label>
				<label>Weights <input bind:value={gWeights} placeholder="400, 700" /></label>
				<button type="submit" disabled={busy}>Install</button>
				<p class="subtle">Downloaded once to this instance; viewers never hit Google.</p>
			</form>
		</div>
	{:else}
		<p class="subtle">Only an admin can install or remove fonts.</p>
	{/if}

	<h2>Installed</h2>
	{#if byFamily.length === 0}
		<p class="subtle">No fonts installed yet.</p>
	{:else}
		<ul class="families">
			{#each byFamily as [family, variants] (family)}
				<li>
					<span class="family" style="font-family: '{family}', sans-serif">{family}</span>
					<div class="variants">
						{#each variants as v (v.id)}
							<span class="variant">
								{v.weight}
								{v.style}
								<small>{v.source}</small>
								{#if isAdmin}
									<button class="x" onclick={() => remove(v)} aria-label="remove">×</button>
								{/if}
							</span>
						{/each}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
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

	.hint code {
		background: rgba(11, 18, 21, 0.08);
		padding: 0.05em 0.3em;
		border-radius: 4px;
	}

	.error {
		color: #b3261e;
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

	.forms {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1.25rem;
	}

	form {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 1rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: #fff;
	}

	form .row {
		display: flex;
		gap: 0.5rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		font-weight: 600;
	}

	input,
	select {
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
		padding: 0.5rem;
		border: none;
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		cursor: pointer;
		align-self: flex-start;
	}

	form button:disabled {
		opacity: 0.6;
	}

	.families {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.families li {
		padding: 0.75rem 1rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.6rem;
		background: #fff;
	}

	.family {
		font-size: 1.25rem;
		font-weight: 700;
	}

	.variants {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-top: 0.5rem;
	}

	.variant {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.8rem;
		padding: 0.2rem 0.5rem;
		border: 1px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.4rem;
	}

	.variant small {
		opacity: 0.5;
	}

	.x {
		border: none;
		background: transparent;
		color: #b3261e;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
		padding: 0;
	}

	@media (max-width: 640px) {
		.forms {
			grid-template-columns: 1fr;
		}
	}
</style>
