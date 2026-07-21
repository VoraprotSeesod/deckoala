<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown as markdownLang } from '@codemirror/lang-markdown';
	import { renderDeck } from '$lib/marp';
	import { api, ApiError, type RevisionMeta } from '$lib/api';

	let { data } = $props();

	// --- server-state baselines ---
	// Initial capture is deliberate: the editor owns this state after mount;
	// the $effect below re-seeds everything when navigation swaps the deck.
	// svelte-ignore state_referenced_locally
	let deckId = $state(data.deck.id);
	// svelte-ignore state_referenced_locally
	let title = $state(data.deck.title);
	// svelte-ignore state_referenced_locally
	let baseline = $state(data.deck.updatedAt); // updatedAt our edits are based on
	// svelte-ignore state_referenced_locally
	let currentMarkdown = $state(data.deck.markdown);

	$effect(() => {
		if (data.deck.id !== deckId) {
			// Route param changed without a remount: reset the whole editor.
			deckId = data.deck.id;
			title = data.deck.title;
			baseline = data.deck.updatedAt;
			currentMarkdown = data.deck.markdown;
			dirty = false;
			saveStatus = 'saved';
			savedAt = '';
			viewingRevision = null;
			panelOpen = false;
			revisions = [];
			saveEpoch += 1;
			if (saveTimer) clearTimeout(saveTimer);
			saveTimer = null;
			setEditorContent(data.deck.markdown);
			renderPreview();
		}
	});

	// --- save state ---
	type SaveStatus = 'saved' | 'dirty' | 'saving' | 'error';
	let saveStatus = $state<SaveStatus>('saved');
	let savedAt = $state('');
	let saveEpoch = 0; // bumped on restore/deck-switch: older saves are void
	let savingInFlight = false;
	let inFlightSave: Promise<void> | null = null;
	let saveTimer: ReturnType<typeof setTimeout> | null = null;
	let dirty = $state(false);

	// --- preview state ---
	let previewHost = $state<HTMLDivElement | null>(null);
	let shadow: ShadowRoot | null = null;
	// Constructable stylesheets: user-influenced CSS (Marp style directives)
	// must never pass through an HTML parser — a literal style-closing tag
	// inside a CSS string would break out of an inline style element and
	// execute the remainder as markup.
	let marpSheet: CSSStyleSheet | null = null;
	let slideCount = $state(0);
	let renderTimer: ReturnType<typeof setTimeout> | null = null;

	// --- revisions state ---
	let panelOpen = $state(false);
	let revisions = $state<RevisionMeta[]>([]);
	let viewingRevision = $state<{ id: string; createdAt: string; markdown: string } | null>(null);

	// --- responsive tabs (below the split-pane breakpoint) ---
	let mobileTab = $state<'write' | 'preview'>('write');

	let errorMsg = $state('');
	let editorContainer = $state<HTMLDivElement | null>(null);
	let view: EditorView | null = null;
	let applyingRemote = false;

	const PREVIEW_CHROME_CSS = `
		:host { display: block; }
		.marpit svg[data-marpit-svg] {
			display: block;
			width: 100%;
			height: auto;
			border-radius: 6px;
			box-shadow: 0 1px 5px rgba(11, 18, 21, 0.15);
			margin: 0 0 1rem;
		}
	`;

	function renderPreview() {
		if (!shadow || !marpSheet) return;
		const source = viewingRevision ? viewingRevision.markdown : currentMarkdown;
		try {
			const { html, css, slideCount: count } = renderDeck(source);
			slideCount = count;
			marpSheet.replaceSync(css);
			// html is safe here: marp renders with html:false (raw HTML escaped).
			shadow.innerHTML = html;
		} catch {
			shadow.innerHTML = `<p style="opacity:.7">Preview failed to render.</p>`;
		}
	}

	function scheduleRender() {
		if (renderTimer) clearTimeout(renderTimer);
		renderTimer = setTimeout(renderPreview, 150);
	}

	function scheduleSave() {
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(() => {
			inFlightSave = saveNow();
		}, 2000);
	}

	async function saveNow() {
		if (!dirty || savingInFlight) return;
		const content = currentMarkdown;
		const epoch = saveEpoch;
		savingInFlight = true;
		saveStatus = 'saving';
		try {
			const updated = await api.decks.update(deckId, {
				markdown: content,
				baseUpdatedAt: baseline
			});
			if (epoch !== saveEpoch) return; // a restore happened meanwhile
			baseline = updated.updatedAt;
			if (currentMarkdown === content) {
				dirty = false;
				saveStatus = 'saved';
				savedAt = new Date().toLocaleTimeString();
			} else {
				saveStatus = 'dirty';
				scheduleSave();
			}
		} catch {
			if (epoch !== saveEpoch) return;
			saveStatus = 'error';
			scheduleSave(); // retry
		} finally {
			savingInFlight = false;
		}
	}

	function onDocChange(content: string) {
		if (applyingRemote) return;
		currentMarkdown = content;
		dirty = true;
		if (saveStatus !== 'saving') saveStatus = 'dirty';
		viewingRevision = null;
		scheduleRender();
		scheduleSave();
	}

	function setEditorContent(content: string) {
		if (!view) return;
		applyingRemote = true;
		view.dispatch({
			changes: { from: 0, to: view.state.doc.length, insert: content }
		});
		applyingRemote = false;
	}

	async function saveTitle() {
		const next = title.trim();
		if (!next || next === data.deck.title) {
			title = next || data.deck.title;
			return;
		}
		// Guard against a late response landing after a deck switch/restore.
		const epoch = saveEpoch;
		const id = deckId;
		errorMsg = '';
		try {
			const updated = await api.decks.update(id, { title: next });
			if (epoch !== saveEpoch || id !== deckId) return;
			title = updated.title;
			baseline = updated.updatedAt;
			data.deck.title = updated.title;
		} catch (e) {
			if (epoch !== saveEpoch || id !== deckId) return;
			errorMsg = e instanceof ApiError ? e.message : 'Rename failed.';
			title = data.deck.title;
		}
	}

	async function togglePanel() {
		panelOpen = !panelOpen;
		if (panelOpen) await refreshRevisions();
	}

	async function refreshRevisions() {
		try {
			revisions = await api.revisions.list(deckId);
		} catch {
			errorMsg = 'Could not load revisions.';
		}
	}

	async function viewRevision(meta: RevisionMeta) {
		errorMsg = '';
		try {
			viewingRevision = await api.revisions.get(deckId, meta.id);
			mobileTab = 'preview';
			renderPreview();
		} catch {
			errorMsg = 'Could not load that revision.';
		}
	}

	function backToCurrent() {
		viewingRevision = null;
		renderPreview();
	}

	async function restoreRevision() {
		if (!viewingRevision) return;
		if (!confirm('Restore this version? Your current content is snapshotted first.')) return;
		// Cancel the pending autosave and void any in-flight PATCH — a queued
		// autosave firing after the restore would silently undo it.
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = null;
		saveEpoch += 1;
		// The in-flight PATCH must fully settle server-side BEFORE the restore
		// POST, or the server could apply them in the opposite order and the
		// stale markdown would overwrite the restored content.
		if (inFlightSave) {
			try {
				await inFlightSave;
			} catch {
				// its failure no longer matters — the restore supersedes it
			}
		}
		errorMsg = '';
		try {
			const updated = await api.revisions.restore(deckId, viewingRevision.id);
			baseline = updated.updatedAt;
			currentMarkdown = updated.markdown;
			dirty = false;
			saveStatus = 'saved';
			savedAt = new Date().toLocaleTimeString();
			viewingRevision = null;
			setEditorContent(updated.markdown);
			renderPreview();
			await refreshRevisions();
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : 'Restore failed.';
			// Put unsaved pre-restore edits back into the autosave loop.
			if (dirty) scheduleSave();
		}
	}

	function sizeLabel(bytes: number): string {
		return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;
	}

	function beforeUnload(event: BeforeUnloadEvent) {
		if (dirty) event.preventDefault();
	}

	// SPA navigation never fires beforeunload — without this, clicking
	// "← Decks" inside the 2s debounce window would silently drop the last
	// edit. Healthy path: flush the latest content in the background and let
	// navigation proceed. Failing-saves path: ask before losing work.
	beforeNavigate((navigation) => {
		if (!dirty && !savingInFlight) return;
		if (saveStatus === 'error') {
			if (!confirm('Your latest changes could not be saved. Leave anyway?')) {
				navigation.cancel();
			}
			return;
		}
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = null;
		const content = currentMarkdown;
		const id = deckId;
		const base = baseline;
		dirty = false;
		void api.decks.update(id, { markdown: content, baseUpdatedAt: base }).catch(() => {
			// Last-ditch flush failed; the revision safety net still holds the
			// previous state, and the user has navigated away by design.
		});
	});

	onMount(() => {
		shadow = previewHost!.attachShadow({ mode: 'open' });
		marpSheet = new CSSStyleSheet();
		const chromeSheet = new CSSStyleSheet();
		chromeSheet.replaceSync(PREVIEW_CHROME_CSS);
		shadow.adoptedStyleSheets = [marpSheet, chromeSheet];
		view = new EditorView({
			parent: editorContainer!,
			doc: currentMarkdown,
			extensions: [
				basicSetup,
				markdownLang(),
				EditorView.lineWrapping,
				EditorView.updateListener.of((update) => {
					if (update.docChanged) onDocChange(update.state.doc.toString());
				})
			]
		});
		renderPreview();
		return () => {
			if (saveTimer) clearTimeout(saveTimer);
			if (renderTimer) clearTimeout(renderTimer);
			view?.destroy();
		};
	});
</script>

<svelte:window onbeforeunload={beforeUnload} />

<svelte:head>
	<title>{title} — Deckoala</title>
</svelte:head>

<article>
	<div class="topbar">
		<a class="button" href="/app">← Decks</a>
		<input
			class="title"
			bind:value={title}
			onblur={saveTitle}
			onkeydown={(e) => {
				if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
			}}
			aria-label="Deck title"
		/>
		<span class="status" data-status={saveStatus}>
			{#if saveStatus === 'saving'}Saving…
			{:else if saveStatus === 'dirty'}Unsaved changes
			{:else if saveStatus === 'error'}Save failed — retrying
			{:else if savedAt}Saved {savedAt}
			{:else}Saved{/if}
		</span>
		<span class="slides">{slideCount} slide{slideCount === 1 ? '' : 's'}</span>
		<a class="button" href={api.decks.exportUrl(deckId)} download>Export .md</a>
		<button class="button" class:active={panelOpen} onclick={togglePanel}>Revisions</button>
	</div>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	<div class="tabs">
		<button class:active={mobileTab === 'write'} onclick={() => (mobileTab = 'write')}>Write</button>
		<button class:active={mobileTab === 'preview'} onclick={() => (mobileTab = 'preview')}>
			Preview
		</button>
	</div>

	<div class="workspace" class:panel-open={panelOpen}>
		<div class="editor" data-tab-active={mobileTab === 'write'}>
			<div class="cm-host" bind:this={editorContainer}></div>
		</div>
		<div class="preview" data-tab-active={mobileTab === 'preview'}>
			{#if viewingRevision}
				<div class="revision-banner">
					Viewing revision from {new Date(viewingRevision.createdAt).toLocaleString()}
					<span>
						<button onclick={restoreRevision}>Restore this version</button>
						<button onclick={backToCurrent}>Back to current</button>
					</span>
				</div>
			{/if}
			<div class="preview-host" bind:this={previewHost}></div>
		</div>
		{#if panelOpen}
			<aside class="revisions">
				<h2>Revisions</h2>
				{#if revisions.length === 0}
					<p class="hint">No snapshots yet — they appear as you keep editing.</p>
				{:else}
					<ul>
						{#each revisions as rev (rev.id)}
							<li>
								<button
									class:selected={viewingRevision?.id === rev.id}
									onclick={() => viewRevision(rev)}
								>
									{new Date(rev.createdAt).toLocaleString()}
									<small>{sizeLabel(rev.sizeBytes)}</small>
								</button>
							</li>
						{/each}
					</ul>
				{/if}
			</aside>
		{/if}
	</div>
</article>

<style>
	article {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		height: calc(100dvh - 8.5rem);
		min-height: 24rem;
	}

	.topbar {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		flex-wrap: wrap;
	}

	.title {
		flex: 1;
		min-width: 10rem;
		font: inherit;
		font-size: 1.15rem;
		font-weight: 700;
		padding: 0.35rem 0.5rem;
		border: 1.5px solid transparent;
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
	}

	.title:hover,
	.title:focus {
		border-color: color-mix(in srgb, var(--dk-ink) 25%, transparent);
		background: #fff;
		outline: none;
	}

	.status {
		font-size: 0.85rem;
		opacity: 0.65;
		white-space: nowrap;
	}

	.status[data-status='error'] {
		color: #b3261e;
		opacity: 1;
	}

	.slides {
		font-size: 0.85rem;
		opacity: 0.65;
		white-space: nowrap;
	}

	.button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.4rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		text-decoration: none;
		cursor: pointer;
	}

	.button.active {
		background: var(--dk-ink);
		color: var(--dk-bg);
	}

	.error {
		color: #b3261e;
		margin: 0;
	}

	.tabs {
		display: none;
		gap: 0.4rem;
	}

	.tabs button {
		flex: 1;
		font: inherit;
		font-weight: 600;
		padding: 0.45rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
	}

	.tabs button.active {
		background: var(--dk-ink);
		color: var(--dk-bg);
	}

	.workspace {
		flex: 1;
		min-height: 0;
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.75rem;
	}

	.workspace.panel-open {
		grid-template-columns: 1fr 1fr 15rem;
	}

	.editor,
	.preview {
		min-width: 0;
		min-height: 0;
		overflow: auto;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: #fff;
	}

	.preview {
		padding: 0.75rem;
		background: color-mix(in srgb, var(--dk-ink) 4%, var(--dk-bg));
	}

	.cm-host {
		height: 100%;
	}

	.cm-host :global(.cm-editor) {
		height: 100%;
		font-size: 0.95rem;
	}

	.cm-host :global(.cm-scroller) {
		font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
	}

	.revision-banner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
		flex-wrap: wrap;
		padding: 0.5rem 0.75rem;
		margin-bottom: 0.75rem;
		border: 1.5px solid #b3862d;
		border-radius: 0.5rem;
		background: #fdf4dd;
		font-size: 0.9rem;
	}

	.revision-banner button {
		font: inherit;
		font-size: 0.8rem;
		font-weight: 600;
		padding: 0.25rem 0.55rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.4rem;
		background: transparent;
		cursor: pointer;
	}

	.revisions {
		overflow: auto;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: #fff;
		padding: 0.75rem;
	}

	.revisions h2 {
		font-size: 1rem;
		margin: 0 0 0.5rem;
	}

	.revisions ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.revisions li button {
		width: 100%;
		display: flex;
		justify-content: space-between;
		gap: 0.5rem;
		font: inherit;
		font-size: 0.82rem;
		padding: 0.4rem 0.5rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
		text-align: left;
	}

	.revisions li button.selected {
		border-color: var(--dk-ink);
		background: color-mix(in srgb, var(--dk-ink) 6%, transparent);
	}

	.revisions small {
		opacity: 0.6;
	}

	.hint {
		font-size: 0.85rem;
		opacity: 0.65;
	}

	@media (max-width: 900px) {
		.tabs {
			display: flex;
		}

		.workspace,
		.workspace.panel-open {
			grid-template-columns: 1fr;
		}

		.editor[data-tab-active='false'],
		.preview[data-tab-active='false'] {
			display: none;
		}

		.revisions {
			max-height: 14rem;
		}
	}
</style>
