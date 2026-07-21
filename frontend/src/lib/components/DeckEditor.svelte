<script lang="ts">
	import { onMount } from 'svelte';
	import type { Snippet } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { EditorView, basicSetup } from 'codemirror';
	import { markdown as markdownLang } from '@codemirror/lang-markdown';
	import { Compartment } from '@codemirror/state';
	import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
	import { tags } from '@lezer/highlight';
	import 'katex/dist/katex.min.css';
	import { renderDeck } from '$lib/marp';
	import { reorderSlides } from '$lib/slides';
	import { api, ApiError, type EditorAdapter, type RevisionMeta } from '$lib/api';
	import { t, formatDate, formatTime, settings } from '$lib/i18n.svelte';

	// CodeMirror carries no dark theme by default, so in the app's dark mode its
	// gutter/caret/syntax colours stay light-tuned on the dark surface. Swap a
	// dark theme + highlight in via a Compartment, reconfigured on theme toggle.
	const cmDarkHighlight = HighlightStyle.define([
		{ tag: tags.heading, color: '#9db8ff', fontWeight: 'bold' },
		{ tag: tags.strong, fontWeight: 'bold', color: '#e7ebef' },
		{ tag: tags.emphasis, fontStyle: 'italic' },
		{ tag: [tags.link, tags.url], color: '#7fb0ff' },
		{ tag: tags.monospace, color: '#8fe3a8' },
		{ tag: [tags.keyword, tags.tagName, tags.propertyName], color: '#c9a6f5' },
		{ tag: tags.string, color: '#8fe3a8' },
		{ tag: [tags.comment, tags.quote], color: '#7f8c9a', fontStyle: 'italic' },
		{ tag: [tags.meta, tags.processingInstruction, tags.labelName], color: '#9aa7b5' }
	]);
	const cmDarkTheme = EditorView.theme(
		{
			'&': { color: '#e7ebef' },
			'.cm-content': { caretColor: '#e7ebef' },
			'.cm-cursor, .cm-dropCursor': { borderLeftColor: '#e7ebef' },
			'.cm-gutters': { backgroundColor: 'transparent', color: '#66727f', border: 'none' },
			'.cm-activeLine': { backgroundColor: 'rgba(255, 255, 255, 0.045)' },
			'.cm-activeLineGutter': { backgroundColor: 'rgba(255, 255, 255, 0.045)', color: '#aab4c0' },
			'.cm-selectionBackground, &.cm-focused .cm-selectionBackground, .cm-content ::selection': {
				backgroundColor: 'rgba(125, 160, 255, 0.28)'
			},
			'.cm-selectionMatch': { backgroundColor: 'rgba(125, 160, 255, 0.18)' }
		},
		{ dark: true }
	);
	const cmTheme = new Compartment();
	const cmThemeExt = (theme: string) =>
		theme === 'dark' ? [cmDarkTheme, syntaxHighlighting(cmDarkHighlight)] : [];

	let {
		deck,
		adapter,
		backHref,
		backLabel,
		presentHref,
		onPresent,
		banner,
		extra,
		aiEnabled = false
	}: {
		deck: { id: string; title: string; markdown: string; updatedAt: string };
		adapter: EditorAdapter;
		backHref: string;
		backLabel?: string;
		presentHref?: string;
		/** Called with the CURRENT (possibly unsaved) markdown to present it. */
		onPresent?: (markdown: string) => void;
		banner?: string;
		/** Optional owner-only controls rendered in the top bar (e.g. Share). */
		extra?: Snippet;
		/** Owner route ONLY. Never set from /s/[token], so an anonymous
		 * share-edit visitor can't spend the instance's AI budget. */
		aiEnabled?: boolean;
	} = $props();

	const back = $derived(backLabel ?? t('editor.backDecks'));

	// --- server-state baselines (the editor owns this state after mount) ---
	// svelte-ignore state_referenced_locally
	let deckId = $state(deck.id);
	// svelte-ignore state_referenced_locally
	let title = $state(deck.title);
	// svelte-ignore state_referenced_locally
	let serverTitle = $state(deck.title); // last title the server confirmed
	// svelte-ignore state_referenced_locally
	let baseline = $state(deck.updatedAt); // updatedAt our edits are based on
	// svelte-ignore state_referenced_locally
	let currentMarkdown = $state(deck.markdown);

	$effect(() => {
		if (deck.id !== deckId) {
			// The parent swapped decks without a remount: reset everything.
			deckId = deck.id;
			title = deck.title;
			serverTitle = deck.title;
			baseline = deck.updatedAt;
			currentMarkdown = deck.markdown;
			dirty = false;
			saveStatus = 'saved';
			savedAtMs = null;
			viewingRevision = null;
			panelOpen = false;
			revisions = [];
			saveEpoch += 1;
			if (saveTimer) clearTimeout(saveTimer);
			saveTimer = null;
			setEditorContent(deck.markdown);
			renderPreview();
		}
	});

	// --- save state ---
	type SaveStatus = 'saved' | 'dirty' | 'saving' | 'error';
	let saveStatus = $state<SaveStatus>('saved');
	// Store the save INSTANT (ms), not a pre-formatted string, so the displayed
	// time re-formats when the locale toggles (formatTime runs in the template).
	let savedAtMs = $state<number | null>(null);
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
	// inside a CSS string would break out of an inline style element.
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

	// --- AI generation (owner route only) ---
	let aiOpen = $state(false);
	let aiPrompt = $state('');
	let aiUseContext = $state(true);
	let aiBusy = $state(false);
	let aiError = $state('');
	let aiResult = $state('');

	/** The model returns a whole deck; when appending we drop its frontmatter so
	 * the existing deck keeps exactly one. */
	function stripFrontmatter(md: string): string {
		const match = md.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/);
		return match ? md.slice(match[0].length).trimStart() : md;
	}

	async function runGenerate() {
		if (!aiPrompt.trim()) return;
		aiBusy = true;
		aiError = '';
		aiResult = '';
		try {
			const res = await api.ai.generate(
				aiPrompt.trim(),
				aiUseContext ? currentMarkdown : undefined
			);
			aiResult = res.markdown;
		} catch (e) {
			aiError = e instanceof ApiError ? e.message : t('ai.failed');
		} finally {
			aiBusy = false;
		}
	}

	function applyAi(mode: 'append' | 'replace') {
		if (!aiResult) return;
		const next =
			mode === 'replace'
				? aiResult
				: `${currentMarkdown.trimEnd()}\n\n---\n\n${stripFrontmatter(aiResult)}\n`;
		applyEditorContent(next);
		closeAi();
	}

	function closeAi() {
		aiOpen = false;
		aiPrompt = '';
		aiResult = '';
		aiError = '';
	}

	let errorMsg = $state('');
	let pdfBusy = $state(false);
	let editorContainer = $state<HTMLDivElement | null>(null);
	let view: EditorView | null = null;
	let applyingRemote = false;

	async function exportPdf() {
		pdfBusy = true;
		errorMsg = '';
		try {
			await adapter.downloadPdf(title);
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : t('editor.pdfFailed');
		} finally {
			pdfBusy = false;
		}
	}

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
			shadow.innerHTML = `<p style="opacity:.7">${t('editor.previewFailed')}</p>`;
		}
	}

	// Clone each rendered slide SVG into its thumbnail's own shadow root, which
	// adopts the SAME marpSheet as the preview so the thumbnail is fully styled
	// without a second render.
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

	// Re-theme CodeMirror when the app theme toggles (the initial theme is set
	// in the extensions at creation).
	$effect(() => {
		const theme = settings.theme;
		view?.dispatch({ effects: cmTheme.reconfigure(cmThemeExt(theme)) });
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

	/** Filenames become alt text — strip Markdown-structural characters. */
	function altText(name: string): string {
		return name.replace(/[[\]()\\\r\n]/g, '').trim() || 'image';
	}

	/** Push the current editor doc into component state + autosave. */
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
		// Guard against the deck being switched (or a restore) mid-upload.
		const id = deckId;
		const epoch = saveEpoch;
		uploading = true;
		errorMsg = '';
		try {
			for (const file of images) {
				const asset = await adapter.uploadAsset(file);
				if (deckId !== id || saveEpoch !== epoch || viewingRevision || !view) return;
				const snippet = `![${altText(asset.originalName)}](${asset.url})\n`;
				const pos = view.state.selection.main.head;
				applyingRemote = true;
				view.dispatch({
					changes: { from: pos, insert: snippet },
					selection: { anchor: pos + snippet.length }
				});
				applyingRemote = false;
				syncFromEditor();
			}
		} catch (e) {
			if (deckId === id) {
				errorMsg = e instanceof ApiError ? e.message : t('editor.imageUploadFailed');
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
			const updated = await adapter.update({ markdown: content, baseUpdatedAt: baseline });
			if (epoch !== saveEpoch) return; // a restore happened meanwhile
			baseline = updated.updatedAt;
			if (currentMarkdown === content) {
				dirty = false;
				saveStatus = 'saved';
				savedAtMs = Date.now();
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
		if (!next || next === serverTitle) {
			title = next || serverTitle;
			return;
		}
		const epoch = saveEpoch;
		errorMsg = '';
		try {
			const updated = await adapter.update({ title: next });
			if (epoch !== saveEpoch) return;
			title = updated.title;
			serverTitle = updated.title;
			baseline = updated.updatedAt;
		} catch (e) {
			if (epoch !== saveEpoch) return;
			errorMsg = e instanceof ApiError ? e.message : t('editor.renameFailed');
			title = serverTitle;
		}
	}

	async function togglePanel() {
		panelOpen = !panelOpen;
		if (panelOpen) await refreshRevisions();
	}

	async function refreshRevisions() {
		try {
			revisions = await adapter.listRevisions();
		} catch {
			errorMsg = t('editor.revisionsLoadFailed');
		}
	}

	async function viewRevision(meta: RevisionMeta) {
		errorMsg = '';
		try {
			viewingRevision = await adapter.getRevision(meta.id);
			mobileTab = 'preview';
			renderPreview();
		} catch {
			errorMsg = t('editor.revisionLoadFailed');
		}
	}

	function backToCurrent() {
		viewingRevision = null;
		renderPreview();
	}

	async function restoreRevision() {
		if (!viewingRevision) return;
		if (!confirm(t('editor.restoreConfirm'))) return;
		// Cancel the pending autosave and void any in-flight PATCH.
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = null;
		saveEpoch += 1;
		// The in-flight PATCH must settle server-side BEFORE the restore POST.
		if (inFlightSave) {
			try {
				await inFlightSave;
			} catch {
				// its failure no longer matters — the restore supersedes it
			}
		}
		errorMsg = '';
		try {
			const updated = await adapter.restoreRevision(viewingRevision.id);
			baseline = updated.updatedAt;
			currentMarkdown = updated.markdown;
			dirty = false;
			saveStatus = 'saved';
			savedAtMs = Date.now();
			viewingRevision = null;
			setEditorContent(updated.markdown);
			renderPreview();
			await refreshRevisions();
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : t('editor.restoreFailed');
			if (dirty) scheduleSave();
		}
	}

	function sizeLabel(bytes: number): string {
		return bytes < 1024 ? `${bytes} B` : `${(bytes / 1024).toFixed(1)} KB`;
	}

	function beforeUnload(event: BeforeUnloadEvent) {
		if (dirty) event.preventDefault();
	}

	// SPA navigation never fires beforeunload — flush the latest content so a
	// nav inside the debounce window can't drop the last edit.
	beforeNavigate((navigation) => {
		if (!dirty && !savingInFlight) return;
		if (saveStatus === 'error') {
			if (!confirm(t('editor.leaveUnsaved'))) {
				navigation.cancel();
			}
			return;
		}
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = null;
		const content = currentMarkdown;
		const base = baseline;
		dirty = false;
		void adapter.update({ markdown: content, baseUpdatedAt: base }).catch(() => {
			// Last-ditch flush failed; the revision safety net still holds.
		});
	});

	const RAIL_CHROME_CSS = `
		:host { display: block; }
		.marpit { display: block; }
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
				cmTheme.of(cmThemeExt(settings.theme)),
				EditorView.updateListener.of((update) => {
					if (update.docChanged) onDocChange(update.state.doc.toString());
				})
			]
		});

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
	{#if banner}<p class="banner">{banner}</p>{/if}
	<div class="topbar">
		<a class="button" href={backHref}>{back}</a>
		<input
			class="title"
			bind:value={title}
			onblur={saveTitle}
			onkeydown={(e) => {
				if (e.key === 'Enter') (e.currentTarget as HTMLInputElement).blur();
			}}
			aria-label={t('editor.deckTitle')}
		/>
		<span class="status" data-status={saveStatus}>
			{#if saveStatus === 'saving'}{t('editor.saving')}
			{:else if saveStatus === 'dirty'}{t('editor.unsaved')}
			{:else if saveStatus === 'error'}{t('editor.saveFailed')}
			{:else if savedAtMs}{t('editor.savedAt', { when: formatTime(savedAtMs) })}
			{:else}{t('editor.saved')}{/if}
		</span>
		<span class="slides">{t('editor.slides', { n: slideCount })}</span>
		{#if presentHref}
			<a class="button" href={presentHref}>{t('editor.present')}</a>
		{:else if onPresent}
			<button class="button" onclick={() => onPresent?.(currentMarkdown)}>{t('editor.present')}</button>
		{/if}
		<button class="button" onclick={exportPdf} disabled={pdfBusy}>
			{pdfBusy ? t('editor.pdfBusy') : t('editor.pdf')}
		</button>
		<a class="button" href={adapter.exportMdUrl} download>{t('editor.exportMd')}</a>
		<button class="button" class:active={panelOpen} onclick={togglePanel}>{t('editor.revisions')}</button>
		{#if aiEnabled}
			<button class="button ai" onclick={() => (aiOpen = true)}>{t('ai.button')}</button>
		{/if}
		{@render extra?.()}
	</div>

	{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

	{#if slideCount > 0}
		<div class="rail" role="listbox" aria-label={t('editor.slidesRail')}>
			{#each Array(slideCount) as _, i (i)}
				<div
					class="thumb"
					class:active={i === activeSlide}
					class:dragging={i === dragFrom}
					draggable={!viewingRevision}
					role="option"
					aria-selected={i === activeSlide}
					tabindex="0"
					title={viewingRevision ? t('editor.reorderDisabled') : t('editor.reorderHint')}
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
		<button class:active={mobileTab === 'write'} onclick={() => (mobileTab = 'write')}>
			{t('editor.write')}
		</button>
		<button class:active={mobileTab === 'preview'} onclick={() => (mobileTab = 'preview')}>
			{t('editor.preview')}
		</button>
	</div>

	<div class="workspace" class:panel-open={panelOpen}>
		<div class="editor" class:drop-active={dropActive} data-tab-active={mobileTab === 'write'}>
			<div class="cm-host" bind:this={editorContainer}></div>
			{#if dropActive}<div class="drop-hint">{t('editor.dropImage')}</div>{/if}
			{#if uploading}<div class="upload-hint">{t('editor.uploading')}</div>{/if}
		</div>
		<div class="preview" data-tab-active={mobileTab === 'preview'}>
			{#if viewingRevision}
				<div class="revision-banner">
					{t('editor.viewingRevision', { when: formatDate(viewingRevision.createdAt) })}
					<span>
						<button onclick={restoreRevision}>{t('editor.restoreThis')}</button>
						<button onclick={backToCurrent}>{t('editor.backToCurrent')}</button>
					</span>
				</div>
			{/if}
			<div class="preview-host" bind:this={previewHost}></div>
		</div>
		{#if panelOpen}
			<aside class="revisions">
				<h2>{t('editor.revisionsHeading')}</h2>
				{#if revisions.length === 0}
					<p class="hint">{t('editor.noSnapshots')}</p>
				{:else}
					<ul>
						{#each revisions as rev (rev.id)}
							<li>
								<button
									class:selected={viewingRevision?.id === rev.id}
									onclick={() => viewRevision(rev)}
								>
									{formatDate(rev.createdAt)}
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

{#if aiOpen}
	<div
		class="ai-overlay"
		role="button"
		tabindex="0"
		aria-label={t('common.close')}
		onclick={closeAi}
		onkeydown={(e) => {
			if (e.key === 'Escape') closeAi();
		}}
	></div>
	<div class="ai-modal" role="dialog" aria-modal="true" aria-label={t('ai.title')}>
		<h2>{t('ai.title')}</h2>
		{#if aiError}<p class="error" role="alert">{aiError}</p>{/if}
		<label class="ai-label">
			{t('ai.promptLabel')}
			<textarea bind:value={aiPrompt} rows="3" placeholder={t('ai.promptPlaceholder')}></textarea>
		</label>
		<label class="ai-check">
			<input type="checkbox" bind:checked={aiUseContext} />
			{t('ai.useContext')}
		</label>
		<div class="ai-actions">
			<button class="button" onclick={runGenerate} disabled={aiBusy || !aiPrompt.trim()}>
				{aiBusy ? t('ai.generating') : t('ai.generate')}
			</button>
			<button class="button" onclick={closeAi}>{t('ai.discard')}</button>
		</div>
		{#if aiResult}
			<h3>{t('ai.result')}</h3>
			<pre class="ai-result">{aiResult}</pre>
			<div class="ai-actions">
				<button class="button" onclick={() => applyAi('append')}>{t('ai.insert')}</button>
				<button class="button" onclick={() => applyAi('replace')}>{t('ai.replace')}</button>
			</div>
		{/if}
	</div>
{/if}

<style>
	.ai-overlay {
		position: fixed;
		inset: 0;
		background: rgba(11, 18, 21, 0.4);
		z-index: 40;
	}

	.ai-modal {
		position: fixed;
		z-index: 41;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(38rem, calc(100vw - 2rem));
		max-height: calc(100dvh - 3rem);
		overflow: auto;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		background: var(--dk-bg);
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.9rem;
		padding: 1.25rem;
		box-shadow: 0 10px 40px rgba(11, 18, 21, 0.25);
	}

	.ai-modal h2 {
		font-size: 1.15rem;
		margin: 0;
	}

	.ai-modal h3 {
		font-size: 0.95rem;
		margin: 0.4rem 0 0;
	}

	.ai-label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.85rem;
		font-weight: 600;
	}

	.ai-label textarea {
		font: inherit;
		font-weight: 400;
		padding: 0.5rem 0.6rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.5rem;
		background: var(--dk-surface);
		color: var(--dk-ink);
		resize: vertical;
	}

	.ai-check {
		display: flex;
		align-items: center;
		gap: 0.45rem;
		font-size: 0.85rem;
	}

	.ai-actions {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.ai-result {
		margin: 0;
		max-height: 16rem;
		overflow: auto;
		padding: 0.6rem 0.75rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.5rem;
		background: var(--dk-surface);
		font-size: 0.8rem;
		white-space: pre-wrap;
		word-break: break-word;
	}

	article {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		height: calc(100dvh - 8.5rem);
		min-height: 24rem;
	}

	.banner {
		margin: 0;
		padding: 0.4rem 0.7rem;
		border-radius: 0.5rem;
		font-size: 0.85rem;
		font-weight: 600;
		background: color-mix(in srgb, var(--dk-ink) 8%, var(--dk-bg));
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 20%, transparent);
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
		background: var(--dk-surface);
		outline: none;
	}

	.status {
		font-size: 0.85rem;
		opacity: 0.65;
		white-space: nowrap;
	}

	.status[data-status='error'] {
		color: var(--dk-danger);
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
		color: var(--dk-danger);
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
		/* A thumbnail is a slide surface, not app chrome — keep it light-branded
		   in both themes (the marp <section> edges are transparent). */
		background: #f8f8ff;
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
		background: var(--dk-surface);
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
		border: 1.5px solid var(--dk-warn);
		border-radius: 0.5rem;
		background: var(--dk-warn-bg);
		color: var(--dk-warn-ink);
		font-size: 0.9rem;
	}

	.revision-banner button {
		font: inherit;
		font-size: 0.8rem;
		font-weight: 600;
		padding: 0.25rem 0.55rem;
		border: 1.5px solid var(--dk-warn);
		border-radius: 0.4rem;
		background: transparent;
		color: var(--dk-warn-ink);
		cursor: pointer;
	}

	.revisions {
		overflow: auto;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 15%, transparent);
		border-radius: 0.75rem;
		background: var(--dk-surface);
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
