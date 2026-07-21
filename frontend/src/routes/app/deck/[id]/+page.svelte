<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown as markdownLang } from '@codemirror/lang-markdown';
	import 'katex/dist/katex.min.css';
	import { renderDeck } from '$lib/marp';
	import { reorderSlides } from '$lib/slides';
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

	// --- slide rail + image upload ---
	let thumbHosts = $state<Array<HTMLDivElement | null>>([]);
	let railChromeSheet: CSSStyleSheet | null = null;
	let railNonce = $state(0); // bumped on each preview render to refresh thumbs
	let dragFrom = $state<number | null>(null);
	let activeSlide = $state(0);
	let uploading = $state(false);
	let dropActive = $state(false);

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
			if (activeSlide >= count) activeSlide = Math.max(0, count - 1);
			marpSheet.replaceSync(css);
			// html is safe here: marp renders with html:false (raw HTML escaped).
			shadow.innerHTML = html;
			railNonce += 1; // triggers the rail effect once the DOM has flushed
		} catch {
			shadow.innerHTML = `<p style="opacity:.7">Preview failed to render.</p>`;
		}
	}

	// Clone each rendered slide SVG into its thumbnail's own shadow root, which
	// adopts the SAME marpSheet as the preview so the thumbnail is fully styled
	// without a second render. Runs in an effect so `thumbHosts` reflects the
	// current slideCount after Svelte flushes the DOM.
	function renderRail() {
		if (!shadow || !marpSheet || !railChromeSheet) return;
		const svgs = shadow.querySelectorAll('svg[data-marpit-svg]');
		for (let i = 0; i < slideCount; i++) {
			const host = thumbHosts[i];
			if (!host) continue;
			let root = host.shadowRoot;
			if (!root) {
				root = host.attachShadow({ mode: 'open' });
				root.adoptedStyleSheets = [marpSheet, railChromeSheet];
			}
			const svg = svgs[i];
			root.innerHTML = svg ? (svg as SVGElement).outerHTML : '';
		}
	}

	$effect(() => {
		railNonce;
		slideCount;
		renderRail();
	});

	/** Apply a programmatic content change (reorder / image insert) AND run the
	 * same state updates onDocChange does — setEditorContent alone suppresses
	 * them via applyingRemote, which would skip autosave + re-render. */
	function applyEditorContent(next: string) {
		if (!view) return;
		applyingRemote = true;
		view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: next } });
		applyingRemote = false;
		currentMarkdown = next;
		dirty = true;
		if (saveStatus !== 'saving') saveStatus = 'dirty';
		viewingRevision = null;
		scheduleRender();
		scheduleSave();
	}

	function onThumbDrop(to: number) {
		const from = dragFrom;
		dragFrom = null;
		if (from === null || from === to || viewingRevision) return;
		applyEditorContent(reorderSlides(currentMarkdown, from, to));
		activeSlide = to;
	}

	function scrollToSlide(i: number) {
		activeSlide = i;
		const svg = shadow?.querySelectorAll('svg[data-marpit-svg]')[i] as SVGElement | undefined;
		svg?.scrollIntoView({ behavior: 'smooth', block: 'start' });
		mobileTab = 'preview';
	}

	/** Filenames become alt text — strip Markdown-structural characters so a
	 * name like `array[0].png` can't break the ![](…) syntax or inject a link. */
	function altText(name: string): string {
		return name.replace(/[[\]()\\\r\n]/g, '').trim() || 'image';
	}

	/** Push the current editor doc into component state + autosave. Called
	 * after each successful image insert so a later failure never strands an
	 * already-inserted (and uploaded) image outside the save loop. */
	function syncFromEditor() {
		if (!view) return;
		currentMarkdown = view.state.doc.toString();
		dirty = true;
		if (saveStatus !== 'saving') saveStatus = 'dirty';
		viewingRevision = null;
		scheduleRender();
		scheduleSave();
	}

	async function uploadAndInsert(files: File[]) {
		const images = files.filter((f) => f.type.startsWith('image/'));
		if (!images.length || !view || viewingRevision) return;
		// Guard against the deck being switched (or a restore) mid-upload: the
		// asset is stored against THIS deck, so it must only be inserted while
		// this deck is still open, mirroring saveNow()/saveTitle().
		const id = deckId;
		const epoch = saveEpoch;
		uploading = true;
		errorMsg = '';
		try {
			for (const file of images) {
				const asset = await api.assets.upload(id, file);
				if (deckId !== id || saveEpoch !== epoch || viewingRevision || !view) return;
				const snippet = `![${altText(asset.originalName)}](${asset.url})\n`;
				const pos = view.state.selection.main.head;
				applyingRemote = true;
				view.dispatch({
					changes: { from: pos, insert: snippet },
					selection: { anchor: pos + snippet.length }
				});
				applyingRemote = false;
				// Sync per image, so a later upload throwing can't lose this one.
				syncFromEditor();
			}
		} catch (e) {
			if (deckId === id) {
				errorMsg = e instanceof ApiError ? e.message : 'Image upload failed.';
			}
		} finally {
			uploading = false;
		}
	}

	function onEditorDrop(event: DragEvent) {
		const files = [...(event.dataTransfer?.files ?? [])];
		if (files.some((f) => f.type.startsWith('image/'))) {
			event.preventDefault();
			dropActive = false;
			void uploadAndInsert(files);
		}
	}

	function onEditorDragOver(event: DragEvent) {
		if (event.dataTransfer?.types.includes('Files')) {
			event.preventDefault();
			dropActive = true;
		}
	}

	function onEditorDragLeave() {
		dropActive = false;
	}

	function onEditorPaste(event: ClipboardEvent) {
		const files = [...(event.clipboardData?.items ?? [])]
			.filter((it) => it.kind === 'file')
			.map((it) => it.getAsFile())
			.filter((f): f is File => f !== null && f.type.startsWith('image/'));
		if (files.length) {
			event.preventDefault();
			void uploadAndInsert(files);
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

	const RAIL_CHROME_CSS = `
		:host { display: block; }
		svg[data-marpit-svg] { display: block; width: 100%; height: auto; }
	`;

	onMount(() => {
		shadow = previewHost!.attachShadow({ mode: 'open' });
		marpSheet = new CSSStyleSheet();
		const chromeSheet = new CSSStyleSheet();
		chromeSheet.replaceSync(PREVIEW_CHROME_CSS);
		shadow.adoptedStyleSheets = [marpSheet, chromeSheet];
		railChromeSheet = new CSSStyleSheet();
		railChromeSheet.replaceSync(RAIL_CHROME_CSS);

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

		// Capture-phase so image drop/paste is intercepted before CodeMirror.
		const host = editorContainer!;
		host.addEventListener('drop', onEditorDrop, true);
		host.addEventListener('dragover', onEditorDragOver, true);
		host.addEventListener('dragleave', onEditorDragLeave);
		host.addEventListener('paste', onEditorPaste, true);

		renderPreview();
		return () => {
			if (saveTimer) clearTimeout(saveTimer);
			if (renderTimer) clearTimeout(renderTimer);
			host.removeEventListener('drop', onEditorDrop, true);
			host.removeEventListener('dragover', onEditorDragOver, true);
			host.removeEventListener('dragleave', onEditorDragLeave);
			host.removeEventListener('paste', onEditorPaste, true);
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

	{#if slideCount > 0}
		<div class="rail" role="listbox" aria-label="Slides">
			{#each Array(slideCount) as _, i (i)}
				<div
					class="thumb"
					class:active={i === activeSlide}
					class:dragging={i === dragFrom}
					draggable={!viewingRevision}
					role="option"
					aria-selected={i === activeSlide}
					tabindex="0"
					title={viewingRevision ? 'Reordering is disabled while viewing a revision' : 'Drag to reorder'}
					ondragstart={() => (dragFrom = i)}
					ondragend={() => (dragFrom = null)}
					ondragover={(e) => e.preventDefault()}
					ondrop={() => onThumbDrop(i)}
					onclick={() => scrollToSlide(i)}
					onkeydown={(e) => {
						if (e.key === 'Enter' || e.key === ' ') {
							e.preventDefault();
							scrollToSlide(i);
						}
					}}
				>
					<div class="thumb-canvas" bind:this={thumbHosts[i]}></div>
					<span class="thumb-n">{i + 1}</span>
				</div>
			{/each}
		</div>
	{/if}

	<div class="tabs">
		<button class:active={mobileTab === 'write'} onclick={() => (mobileTab = 'write')}>Write</button>
		<button class:active={mobileTab === 'preview'} onclick={() => (mobileTab = 'preview')}>
			Preview
		</button>
	</div>

	<div class="workspace" class:panel-open={panelOpen}>
		<div class="editor" class:drop-active={dropActive} data-tab-active={mobileTab === 'write'}>
			<div class="cm-host" bind:this={editorContainer}></div>
			{#if dropActive}<div class="drop-hint">Drop image to upload</div>{/if}
			{#if uploading}<div class="upload-hint">Uploading…</div>{/if}
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

	.rail {
		display: flex;
		gap: 0.5rem;
		overflow-x: auto;
		padding: 0.25rem 0.1rem 0.5rem;
		flex: 0 0 auto;
	}

	.thumb {
		position: relative;
		flex: 0 0 auto;
		width: 8.5rem;
		aspect-ratio: 16 / 9;
		border: 2px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.4rem;
		overflow: hidden;
		cursor: grab;
		background: #fff;
	}

	.thumb.active {
		border-color: var(--dk-ink);
	}

	.thumb.dragging {
		opacity: 0.4;
	}

	.thumb-canvas {
		width: 100%;
		height: 100%;
		pointer-events: none;
	}

	.thumb-n {
		position: absolute;
		bottom: 2px;
		right: 4px;
		font-size: 0.7rem;
		font-weight: 700;
		color: var(--dk-ink);
		background: color-mix(in srgb, var(--dk-bg) 80%, transparent);
		border-radius: 0.25rem;
		padding: 0 0.25rem;
		pointer-events: none;
	}

	.editor,
	.preview {
		position: relative;
		min-width: 0;
		min-height: 0;
		overflow: auto;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: #fff;
	}

	.editor.drop-active {
		outline: 2.5px dashed var(--dk-ink);
		outline-offset: -4px;
	}

	.drop-hint,
	.upload-hint {
		position: absolute;
		top: 0.5rem;
		left: 50%;
		transform: translateX(-50%);
		z-index: 5;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.3rem 0.7rem;
		border-radius: 0.5rem;
		background: var(--dk-ink);
		color: var(--dk-bg);
		pointer-events: none;
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
