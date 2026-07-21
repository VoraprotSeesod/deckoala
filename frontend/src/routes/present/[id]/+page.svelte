<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { renderDeck } from '$lib/marp';
	import { t } from '$lib/i18n.svelte';

	let { data } = $props();
	// Present is a snapshot taken at mount; the window is always opened fresh
	// (link / window.open), never navigated deck-to-deck in place.
	// svelte-ignore state_referenced_locally
	const deckId = data.deck.id;
	const presenterMode = page.url.searchParams.has('presenter');

	// Contain-scale the slide to its own intrinsic aspect ratio (the SVG's
	// viewBox), so 16:9 / 4:3 / any ratio letterboxes without distortion. The
	// slide SVG is wrapped in `.marpit` (its render context) so the theme's
	// section styling — background, sizing — applies.
	const STAGE_CSS = `
		:host { display: block; width: 100%; height: 100%; }
		.marpit { width: 100%; height: 100%; }
		svg[data-marpit-svg] { display: block; width: 100%; height: 100%; }
	`;

	// --- render (once) ---
	let slideSvgs: string[] = [];
	let marpSheet: CSSStyleSheet | null = null;
	let chromeSheet: CSSStyleSheet | null = null;
	let slideCount = $state(0);
	let notes = $state<string[]>([]);
	let ready = $state(false);
	let emptyDeck = $state(false);

	// --- navigation / sync ---
	let index = $state(0);
	let synced = false; // adopt an incoming `state` only until first sync
	let channel: BroadcastChannel | null = null;

	// --- fullscreen (audience) ---
	let isFullscreen = $state(false);
	let lastFsExit = 0;
	let rootEl = $state<HTMLDivElement | null>(null);

	// --- presenter timer ---
	let elapsed = $state(0);
	let timerRunning = $state(true);
	let timerId: ReturnType<typeof setInterval> | null = null;

	// --- audience control bar (idle auto-hide; works on touch, unlike :hover) ---
	let controlsVisible = $state(true);
	let idleTimer: ReturnType<typeof setTimeout> | null = null;
	let notice = $state('');

	function pokeControls() {
		controlsVisible = true;
		if (idleTimer) clearTimeout(idleTimer);
		idleTimer = setTimeout(() => (controlsVisible = false), 2000);
	}

	// Re-enable keyboard nav after a click: a focused button would otherwise
	// swallow the arrow keys via the interactive-target guard.
	function blurTarget(event: Event) {
		(event.currentTarget as HTMLElement | null)?.blur();
	}

	// --- shadow hosts ---
	let stageHost = $state<HTMLDivElement | null>(null);
	let curHost = $state<HTMLDivElement | null>(null);
	let nextHost = $state<HTMLDivElement | null>(null);

	function clamp(i: number): number {
		return Math.max(0, Math.min(i, slideCount - 1));
	}

	function setIndex(i: number, broadcast: boolean) {
		const nextIndex = clamp(i);
		synced = true;
		if (nextIndex === index) return;
		index = nextIndex;
		if (broadcast) channel?.postMessage({ type: 'nav', index });
	}

	function paint(host: HTMLDivElement | null, svgHtml: string | undefined) {
		if (!host || !marpSheet || !chromeSheet) return;
		const root = host.shadowRoot ?? host.attachShadow({ mode: 'open' });
		if (root.adoptedStyleSheets.length === 0) {
			root.adoptedStyleSheets = [marpSheet, chromeSheet];
		}
		// Wrap in `.marpit` so theme rules scoped to the render context apply.
		root.innerHTML = svgHtml ? `<div class="marpit">${svgHtml}</div>` : '';
	}

	// Repaint the visible surfaces whenever the index (or readiness) changes.
	$effect(() => {
		index;
		if (!ready || emptyDeck) return;
		if (presenterMode) {
			paint(curHost, slideSvgs[index]);
			paint(nextHost, slideSvgs[index + 1]);
		} else {
			paint(stageHost, slideSvgs[index]);
		}
	});

	function toggleFullscreen() {
		if (document.fullscreenElement) {
			void document.exitFullscreen();
		} else {
			void rootEl?.requestFullscreen?.();
		}
	}

	function onFullscreenChange() {
		const fs = !!document.fullscreenElement;
		if (isFullscreen && !fs) lastFsExit = Date.now();
		isFullscreen = fs;
	}

	function leave() {
		if (presenterMode) {
			// Works when script-opened; a no-op for a directly-navigated window,
			// so fall back to the editor if we're still here a tick later.
			window.close();
			setTimeout(() => void goto(`/app/deck/${deckId}`), 150);
		} else {
			void goto(`/app/deck/${deckId}`);
		}
	}

	function handleEscape() {
		// The UA already used Esc to exit fullscreen; don't also navigate. The
		// cooldown covers the fullscreenchange event firing after this keydown.
		if (isFullscreen || Date.now() - lastFsExit < 300) return;
		leave();
	}

	function onKeydown(event: KeyboardEvent) {
		const target = event.target as HTMLElement | null;
		const interactive =
			!!target && ['BUTTON', 'INPUT', 'TEXTAREA', 'A', 'SELECT'].includes(target.tagName);
		switch (event.key) {
			case 'ArrowRight':
			case ' ':
			case 'PageDown':
			case 'n':
				if (interactive) return;
				event.preventDefault();
				setIndex(index + 1, true);
				break;
			case 'ArrowLeft':
			case 'PageUp':
			case 'p':
				if (interactive) return;
				event.preventDefault();
				setIndex(index - 1, true);
				break;
			case 'Home':
				if (interactive) return;
				event.preventDefault();
				setIndex(0, true);
				break;
			case 'End':
				if (interactive) return;
				event.preventDefault();
				setIndex(slideCount - 1, true);
				break;
			case 'f':
				if (!presenterMode && !interactive) {
					event.preventDefault();
					toggleFullscreen();
				}
				break;
			case 'Escape':
				handleEscape();
				break;
		}
	}

	// --- touch swipe (audience) ---
	let touchX = 0;
	function onTouchStart(event: TouchEvent) {
		touchX = event.changedTouches[0].clientX;
	}
	function onTouchEnd(event: TouchEvent) {
		const dx = event.changedTouches[0].clientX - touchX;
		if (Math.abs(dx) > 50) setIndex(index + (dx < 0 ? 1 : -1), true);
	}

	function openPresenter() {
		const win = window.open(
			`/present/${deckId}?presenter#${index}`,
			`deckoala-presenter-${deckId}`,
			'width=1100,height=720'
		);
		notice = win ? '' : t('present.popupBlocked');
	}

	function timerLabel(): string {
		const m = Math.floor(elapsed / 60)
			.toString()
			.padStart(2, '0');
		const s = (elapsed % 60).toString().padStart(2, '0');
		return `${m}:${s}`;
	}

	onMount(() => {
		const rendered = renderDeck(data.deck.markdown);
		slideCount = rendered.slideCount;
		notes = rendered.notes;
		emptyDeck = slideCount === 0;
		if (!emptyDeck) {
			marpSheet = new CSSStyleSheet();
			marpSheet.replaceSync(rendered.css);
			chromeSheet = new CSSStyleSheet();
			chromeSheet.replaceSync(STAGE_CSS);
			const tmpl = document.createElement('template');
			tmpl.innerHTML = rendered.html; // safe: marp html:false
			slideSvgs = [...tmpl.content.querySelectorAll('svg[data-marpit-svg]')].map(
				(svg) => svg.outerHTML
			);
		}

		// Seed the initial paint from the URL hash (the opener passes it), but
		// stay UNSYNCED and still run the handshake — a nav the audience made
		// during this window's open latency isn't retained by BroadcastChannel,
		// so the `state` reply must be allowed to correct the seed.
		const hash = page.url.hash.replace('#', '');
		if (/^\d+$/.test(hash)) {
			index = clamp(parseInt(hash, 10));
		}

		channel = new BroadcastChannel(`deckoala-present-${deckId}`);
		channel.onmessage = (event) => {
			const msg = event.data;
			// BroadcastChannel never delivers to the sender, so a window never
			// hears its own hello/nav.
			if (msg?.type === 'hello') {
				channel?.postMessage({ type: 'state', index });
			} else if (msg?.type === 'state') {
				if (!synced) {
					index = clamp(msg.index);
					synced = true;
				}
			} else if (msg?.type === 'nav') {
				index = clamp(msg.index);
				synced = true;
			}
		};
		// Always ask existing windows for the current position.
		channel.postMessage({ type: 'hello' });

		window.addEventListener('keydown', onKeydown);
		document.addEventListener('fullscreenchange', onFullscreenChange);
		ready = true;
		if (presenterMode) {
			timerId = setInterval(() => {
				if (timerRunning) elapsed += 1;
			}, 1000);
		} else {
			pokeControls();
		}

		return () => {
			window.removeEventListener('keydown', onKeydown);
			document.removeEventListener('fullscreenchange', onFullscreenChange);
			channel?.close();
			if (timerId) clearInterval(timerId);
			if (idleTimer) clearTimeout(idleTimer);
		};
	});
</script>

<svelte:head>
	<title>{data.deck.title} — {presenterMode ? t('present.presenterView') : t('present.title')}</title>
</svelte:head>

{#if emptyDeck}
	<div class="empty">
		<p>{t('present.noSlides')}</p>
		<a class="btn" href="/app/deck/{deckId}">{t('present.backToEditor')}</a>
	</div>
{:else if presenterMode}
	<div class="presenter" bind:this={rootEl}>
		<div class="stage-pair">
			<div class="pane">
				<span class="pane-label">{t('present.current', { i: index + 1, n: slideCount })}</span>
				<div class="surface" bind:this={curHost}></div>
			</div>
			<div class="pane">
				<span class="pane-label">{t('present.next')}</span>
				{#if index + 1 < slideCount}
					<div class="surface" bind:this={nextHost}></div>
				{:else}
					<div class="end">{t('present.endOfDeck')}</div>
				{/if}
			</div>
		</div>
		<div class="notes">
			<h2>{t('present.speakerNotes')}</h2>
			<p>{notes[index]?.trim() || t('present.noNotes')}</p>
		</div>
		<div class="controls">
			<div class="timer">{timerLabel()}</div>
			<button onclick={(e) => { timerRunning = !timerRunning; blurTarget(e); }}>
				{timerRunning ? t('present.pause') : t('present.resume')}
			</button>
			<button onclick={(e) => { elapsed = 0; blurTarget(e); }}>{t('present.reset')}</button>
			<span class="spacer"></span>
			<button
				onclick={(e) => {
					setIndex(index - 1, true);
					blurTarget(e);
				}}
				disabled={index === 0}>{t('present.prev')}</button
			>
			<button
				onclick={(e) => {
					setIndex(index + 1, true);
					blurTarget(e);
				}}
				disabled={index + 1 >= slideCount}>{t('present.next')}</button
			>
			<button onclick={leave}>{t('present.exit')}</button>
		</div>
	</div>
{:else}
	<div
		class="audience"
		bind:this={rootEl}
		role="application"
		aria-label={t('present.slideRegion')}
		onpointermove={pokeControls}
		ontouchstart={(e) => {
			pokeControls();
			onTouchStart(e);
		}}
		ontouchend={onTouchEnd}
	>
		<div class="stage" bind:this={stageHost}></div>
		<div class="bar" class:hidden={!controlsVisible}>
			<a class="btn" href="/app/deck/{deckId}">{t('present.exit')}</a>
			<span class="counter">{index + 1} / {slideCount}</span>
			{#if notice}<span class="notice">{notice}</span>{/if}
			<span class="spacer"></span>
			<button
				class="btn"
				onclick={(e) => {
					setIndex(index - 1, true);
					blurTarget(e);
				}}
				disabled={index === 0}>‹</button
			>
			<button
				class="btn"
				onclick={(e) => {
					setIndex(index + 1, true);
					blurTarget(e);
				}}
				disabled={index + 1 >= slideCount}>›</button
			>
			<button class="btn" onclick={(e) => { openPresenter(); blurTarget(e); }}>{t('present.presenterView')}</button>
			<button class="btn" onclick={(e) => { toggleFullscreen(); blurTarget(e); }}>
				{isFullscreen ? t('present.windowed') : t('present.fullscreen')}
			</button>
		</div>
	</div>
{/if}

<style>
	.audience {
		position: fixed;
		inset: 0;
		background: #0b1215;
		display: flex;
		flex-direction: column;
	}

	.stage {
		flex: 1;
		min-height: 0;
		padding: 2.5vmin;
	}

	.bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.75rem;
		background: rgba(11, 18, 21, 0.9);
		color: #f8f8ff;
		transition: opacity 0.3s;
		flex-wrap: wrap;
	}

	.bar.hidden {
		opacity: 0;
		pointer-events: none;
	}

	.notice {
		font-size: 0.85rem;
		color: #ffd479;
	}

	.counter {
		font-size: 0.9rem;
		font-variant-numeric: tabular-nums;
	}

	.spacer {
		flex: 1;
	}

	.btn {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.35rem 0.7rem;
		border: 1.5px solid #f8f8ff;
		border-radius: 0.5rem;
		background: transparent;
		color: #f8f8ff;
		cursor: pointer;
		text-decoration: none;
	}

	.btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.empty {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		background: var(--dk-bg);
	}

	.empty .btn {
		border-color: var(--dk-ink);
		color: var(--dk-ink);
	}

	/* presenter */
	.presenter {
		min-height: 100dvh;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0.75rem;
		background: #0b1215;
		color: #f8f8ff;
	}

	.stage-pair {
		display: grid;
		grid-template-columns: 2fr 1fr;
		gap: 0.75rem;
		min-height: 0;
		flex: 1;
	}

	.pane {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		min-height: 0;
	}

	.pane-label {
		font-size: 0.8rem;
		opacity: 0.7;
	}

	.surface,
	.end {
		flex: 1;
		min-height: 0;
		background: #f8f8ff;
		border-radius: 0.5rem;
		overflow: hidden;
	}

	.end {
		display: flex;
		align-items: center;
		justify-content: center;
		color: #0b1215;
		font-weight: 600;
	}

	.notes {
		background: rgba(248, 248, 255, 0.08);
		border-radius: 0.5rem;
		padding: 0.6rem 0.9rem;
		max-height: 22vh;
		overflow: auto;
	}

	.notes h2 {
		font-size: 0.85rem;
		opacity: 0.7;
		margin: 0 0 0.35rem;
	}

	.notes p {
		margin: 0;
		white-space: pre-wrap;
		line-height: 1.5;
	}

	.controls {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.controls button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.35rem 0.7rem;
		border: 1.5px solid #f8f8ff;
		border-radius: 0.5rem;
		background: transparent;
		color: #f8f8ff;
		cursor: pointer;
	}

	.controls button:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.timer {
		font-size: 1.4rem;
		font-weight: 700;
		font-variant-numeric: tabular-nums;
	}

	@media (max-width: 700px) {
		.stage-pair {
			grid-template-columns: 1fr;
		}
	}
</style>
