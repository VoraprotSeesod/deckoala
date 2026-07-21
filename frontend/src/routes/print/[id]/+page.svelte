<script lang="ts">
	import { onMount, tick } from 'svelte';
	import 'katex/dist/katex.min.css';
	import { renderDeck } from '$lib/marp';

	let { data } = $props();

	// Each slide fills its own PDF page; the marp/theme CSS lives in a
	// per-page shadow root (constructable stylesheet — never HTML-parsed, so a
	// user style directive can't break out), and @page is a self-authored
	// document rule sized to the slide's own dimensions.
	const PAGE_CSS = `
		:host { display: block; width: 100%; height: 100%; }
		.marpit { width: 100%; height: 100%; }
		svg[data-marpit-svg] { display: block; width: 100%; height: 100%; }
	`;

	let slideCount = $state(0);
	let pageHosts = $state<Array<HTMLDivElement | null>>([]);
	let slideSvgs: string[] = [];
	let marpSheet: CSSStyleSheet | null = null;
	let pageSheet: CSSStyleSheet | null = null;

	onMount(async () => {
		const rendered = renderDeck(data.deck.markdown);
		slideCount = rendered.slideCount;
		marpSheet = new CSSStyleSheet();
		marpSheet.replaceSync(rendered.css);
		pageSheet = new CSSStyleSheet();
		pageSheet.replaceSync(PAGE_CSS);

		const tmpl = document.createElement('template');
		tmpl.innerHTML = rendered.html; // safe: marp html:false
		const svgs = [...tmpl.content.querySelectorAll('svg[data-marpit-svg]')] as SVGElement[];
		slideSvgs = svgs.map((s) => s.outerHTML);

		// Page size from the first slide's intrinsic viewBox (e.g. 1280x720, or
		// 960x720 for a `size: 4:3` deck) so the PDF page matches the slide.
		const viewBox = svgs[0]?.getAttribute('viewBox')?.split(/\s+/).map(Number);
		const w = viewBox && viewBox.length === 4 ? viewBox[2] : 1280;
		const h = viewBox && viewBox.length === 4 ? viewBox[3] : 720;

		const style = document.createElement('style');
		style.textContent = `
			@page { size: ${w}px ${h}px; margin: 0; }
			html, body { margin: 0; padding: 0; background: #fff; }
			.page { width: ${w}px; height: ${h}px; overflow: hidden; break-after: page; }
			.page:last-child { break-after: auto; }
		`;
		document.head.appendChild(style);

		// Wait for Svelte to create the {#each} page hosts before painting —
		// a bare microtask isn't guaranteed to run after the DOM update.
		await tick();
		for (let i = 0; i < slideCount; i++) {
			const host = pageHosts[i];
			if (!host) continue;
			const root = host.shadowRoot ?? host.attachShadow({ mode: 'open' });
			if (root.adoptedStyleSheets.length === 0) {
				root.adoptedStyleSheets = [marpSheet, pageSheet];
			}
			root.innerHTML = `<div class="marpit">${slideSvgs[i]}</div>`;
		}

		// Readiness for the printer: fonts AND images must finish, or the PDF
		// prints without them. document.fonts.ready does NOT await images.
		await document.fonts.ready;
		const images: HTMLImageElement[] = [];
		for (const host of pageHosts) {
			if (host?.shadowRoot) images.push(...host.shadowRoot.querySelectorAll('img'));
		}
		await Promise.all(
			images.map((img) =>
				img.complete
					? Promise.resolve()
					: img.decode?.().catch(() => undefined) ??
						new Promise<void>((resolve) => {
							img.onload = img.onerror = () => resolve();
						})
			)
		);
		await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
		(window as unknown as { __DECKOALA_PRINT_READY?: boolean }).__DECKOALA_PRINT_READY = true;
	});
</script>

<svelte:head>
	<title>{data.deck.title}</title>
</svelte:head>

{#each Array(slideCount) as _, i (i)}
	<div class="page" bind:this={pageHosts[i]}></div>
{/each}
