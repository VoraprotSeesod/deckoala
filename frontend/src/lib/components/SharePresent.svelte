<script lang="ts">
	import { onMount } from 'svelte';
	import { renderDeck } from '$lib/marp';

	// Audience-only fullscreen presentation for a shared deck (no presenter view,
	// no cross-window sync — the recipient just clicks through). Reuses the same
	// `.marpit`-wrapped single-slide paint as /present/[id].
	let { markdown, onExit }: { markdown: string; onExit: () => void } = $props();

	const STAGE_CSS = `
		:host { display: block; width: 100%; height: 100%; }
		.marpit { width: 100%; height: 100%; }
		svg[data-marpit-svg] { display: block; width: 100%; height: 100%; }
	`;

	let slideSvgs: string[] = [];
	let marpSheet: CSSStyleSheet | null = null;
	let chromeSheet: CSSStyleSheet | null = null;
	let slideCount = $state(0);
	let ready = $state(false);

	let index = $state(0);
	let isFullscreen = $state(false);
	let rootEl = $state<HTMLDivElement | null>(null);
	let stageHost = $state<HTMLDivElement | null>(null);
	let controlsVisible = $state(true);
	let idleTimer: ReturnType<typeof setTimeout> | null = null;

	function clamp(i: number): number {
		return Math.max(0, Math.min(i, slideCount - 1));
	}

	function go(i: number) {
		index = clamp(i);
	}

	function paint() {
		const host = stageHost;
		if (!host || !marpSheet || !chromeSheet) return;
		const root = host.shadowRoot ?? host.attachShadow({ mode: 'open' });
		if (root.adoptedStyleSheets.length === 0) {
			root.adoptedStyleSheets = [marpSheet, chromeSheet];
		}
		const svg = slideSvgs[index];
		root.innerHTML = svg ? `<div class="marpit">${svg}</div>` : '';
	}

	$effect(() => {
		index;
		if (ready) paint();
	});

	// Move focus to the overlay once it mounts. Otherwise the still-focused
	// "Present" button that opened it (an interactive tagName, sitting behind
	// the fixed overlay) makes every keyboard shortcut a no-op — onKeydown is a
	// window listener that early-returns when document.activeElement is a button.
	let focused = false;
	$effect(() => {
		if (ready && rootEl && !focused) {
			focused = true;
			rootEl.focus();
		}
	});

	function pokeControls() {
		controlsVisible = true;
		if (idleTimer) clearTimeout(idleTimer);
		idleTimer = setTimeout(() => (controlsVisible = false), 2000);
	}

	function blurTarget(event: Event) {
		(event.currentTarget as HTMLElement | null)?.blur();
	}

	function toggleFullscreen() {
		if (document.fullscreenElement) void document.exitFullscreen();
		else void rootEl?.requestFullscreen?.();
	}

	function onFullscreenChange() {
		isFullscreen = !!document.fullscreenElement;
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
				go(index + 1);
				break;
			case 'ArrowLeft':
			case 'PageUp':
			case 'p':
				if (interactive) return;
				event.preventDefault();
				go(index - 1);
				break;
			case 'Home':
				if (interactive) return;
				event.preventDefault();
				go(0);
				break;
			case 'End':
				if (interactive) return;
				event.preventDefault();
				go(slideCount - 1);
				break;
			case 'f':
				if (!interactive) {
					event.preventDefault();
					toggleFullscreen();
				}
				break;
			case 'Escape':
				if (!document.fullscreenElement) onExit();
				break;
		}
	}

	let touchX = 0;
	function onTouchStart(event: TouchEvent) {
		touchX = event.changedTouches[0].clientX;
	}
	function onTouchEnd(event: TouchEvent) {
		const dx = event.changedTouches[0].clientX - touchX;
		if (Math.abs(dx) > 50) go(index + (dx < 0 ? 1 : -1));
	}

	onMount(() => {
		const rendered = renderDeck(markdown);
		slideCount = rendered.slideCount;
		if (slideCount > 0) {
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
		window.addEventListener('keydown', onKeydown);
		document.addEventListener('fullscreenchange', onFullscreenChange);
		ready = true;
		pokeControls();
		return () => {
			window.removeEventListener('keydown', onKeydown);
			document.removeEventListener('fullscreenchange', onFullscreenChange);
			if (idleTimer) clearTimeout(idleTimer);
		};
	});
</script>

{#if slideCount === 0}
	<div class="empty">
		<p>This deck has no slides.</p>
		<button class="btn" onclick={onExit}>Close</button>
	</div>
{:else}
	<div
		class="audience"
		bind:this={rootEl}
		role="application"
		aria-label="Slide presentation"
		tabindex="-1"
		onpointermove={pokeControls}
		ontouchstart={(e) => {
			pokeControls();
			onTouchStart(e);
		}}
		ontouchend={onTouchEnd}
	>
		<div class="stage" bind:this={stageHost}></div>
		<div class="bar" class:hidden={!controlsVisible}>
			<button class="btn" onclick={onExit}>Exit</button>
			<span class="counter">{index + 1} / {slideCount}</span>
			<span class="spacer"></span>
			<button class="btn" onclick={(e) => (go(index - 1), blurTarget(e))} disabled={index === 0}>‹</button>
			<button
				class="btn"
				onclick={(e) => (go(index + 1), blurTarget(e))}
				disabled={index + 1 >= slideCount}>›</button
			>
			<button class="btn" onclick={(e) => (toggleFullscreen(), blurTarget(e))}>
				{isFullscreen ? 'Windowed' : 'Fullscreen'}
			</button>
		</div>
	</div>
{/if}

<style>
	.audience {
		position: fixed;
		inset: 0;
		z-index: 50;
		background: #0b1215;
		display: flex;
		flex-direction: column;
	}

	.audience:focus {
		outline: none;
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
	}

	.btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.empty {
		position: fixed;
		inset: 0;
		z-index: 50;
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
</style>
