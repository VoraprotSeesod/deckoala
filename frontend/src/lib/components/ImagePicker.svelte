<script lang="ts">
	import { tick } from 'svelte';
	import { ApiError, type DeckAsset, type EditorAdapter } from '$lib/api';
	import { imageMarkdown, sanitizeAlt, type ImageMode, type ImageSize, type BgVariant } from '$lib/slide-syntax';
	import { t } from '$lib/i18n.svelte';

	type Props = {
		open: boolean;
		adapter: EditorAdapter;
		/** Insert the built markdown at the editor cursor. */
		onInsert: (markdown: string) => void;
		onClose: () => void;
	};
	let { open, adapter, onInsert, onClose }: Props = $props();

	let panel = $state<HTMLDivElement | null>(null);
	let restoreTo: HTMLElement | null = null;

	let assets = $state<DeckAsset[]>([]);
	let loading = $state(false);
	let errorMsg = $state('');
	let fileInput = $state<HTMLInputElement | null>(null);

	// The image currently being configured for insertion (uploaded or reused).
	let chosen = $state<{ url: string; name: string } | null>(null);
	let alt = $state('');
	let mode = $state<ImageMode>('inline');
	let size = $state<ImageSize>('medium');
	let bgVariant = $state<BgVariant>('default');

	async function loadAssets() {
		loading = true;
		errorMsg = '';
		try {
			assets = await adapter.listAssets();
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : t('common.somethingWrong');
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		if (!open) return;
		restoreTo = document.activeElement as HTMLElement | null;
		const prev = document.body.style.overflow;
		document.body.style.overflow = 'hidden';
		chosen = null;
		void loadAssets();
		tick().then(() => panel?.focus());
		return () => {
			document.body.style.overflow = prev;
			restoreTo?.focus?.();
			restoreTo = null;
		};
	});

	function choose(url: string, name: string) {
		chosen = { url, name };
		alt = sanitizeAlt(name);
		mode = 'inline';
		size = 'medium';
		bgVariant = 'default';
	}

	async function onUpload(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		if (!file.type.startsWith('image/')) {
			errorMsg = t('picker.notImage');
			return;
		}
		loading = true;
		errorMsg = '';
		try {
			const uploaded = await adapter.uploadAsset(file);
			await loadAssets();
			choose(uploaded.url, uploaded.originalName);
		} catch (e) {
			errorMsg = e instanceof ApiError ? e.message : t('editor.imageUploadFailed');
		} finally {
			loading = false;
		}
	}

	function insert() {
		if (!chosen) return;
		onInsert(imageMarkdown({ url: chosen.url, alt, mode, size, bgVariant }));
		onClose();
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Escape') {
			event.preventDefault();
			onClose();
		}
	}
</script>

{#if open}
	<div class="scrim">
		<button type="button" class="backdrop" aria-label={t('help.close')} onclick={onClose}></button>
		<div
			class="panel"
			role="dialog"
			aria-modal="true"
			aria-label={t('picker.title')}
			tabindex="-1"
			bind:this={panel}
			onkeydown={onKeydown}
		>
			<header>
				<h2>{t('picker.title')}</h2>
				<button type="button" onclick={onClose}>{t('help.close')}</button>
			</header>

			{#if errorMsg}<p class="error" role="alert">{errorMsg}</p>{/if}

			{#if chosen}
				<!-- Configure the chosen image before inserting. -->
				<div class="configure">
					<img src={chosen.url} alt="" class="preview" />
					{#if mode === 'inline'}
						<!-- Background images (![bg]) carry no alt text, so only offer it inline. -->
						<label>
							{t('picker.alt')}
							<input bind:value={alt} />
						</label>
					{/if}
					<div class="modes">
						<button type="button" class:active={mode === 'inline'} onclick={() => (mode = 'inline')}>
							{t('picker.inline')}
						</button>
						<button type="button" class:active={mode === 'background'} onclick={() => (mode = 'background')}>
							{t('picker.background')}
						</button>
					</div>
					{#if mode === 'inline'}
						<div class="opts">
							{#each ['small', 'medium', 'full'] as s (s)}
								<button type="button" class:active={size === s} onclick={() => (size = s as ImageSize)}>
									{t(`picker.size.${s}`)}
								</button>
							{/each}
						</div>
					{:else}
						<div class="opts">
							{#each ['default', 'fit', 'left', 'right'] as v (v)}
								<button type="button" class:active={bgVariant === v} onclick={() => (bgVariant = v as BgVariant)}>
									{t(`picker.bg.${v}`)}
								</button>
							{/each}
						</div>
					{/if}
					<div class="footer">
						<button type="button" class="ghost" onclick={() => (chosen = null)}>{t('picker.back')}</button>
						<button type="button" class="primary" onclick={insert}>{t('picker.insert')}</button>
					</div>
				</div>
			{:else}
				<div class="browse">
					<button type="button" class="upload" onclick={() => fileInput?.click()} disabled={loading}>
						{t('picker.upload')}
					</button>
					<input
						bind:this={fileInput}
						type="file"
						accept="image/png,image/jpeg,image/gif,image/webp"
						hidden
						onchange={onUpload}
					/>
					<h3>{t('picker.reuse')}</h3>
					{#if loading}
						<p class="subtle">{t('picker.loading')}</p>
					{:else if assets.length === 0}
						<p class="subtle">{t('picker.empty')}</p>
					{:else}
						<ul class="grid">
							{#each assets as asset (asset.id)}
								<li>
									<button type="button" onclick={() => choose(asset.url, asset.originalName)}>
										<img src={asset.url} alt={asset.originalName} loading="lazy" />
										<span>{asset.originalName}</span>
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 60;
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding: max(0.75rem, env(safe-area-inset-top)) 0.75rem 0.75rem;
	}

	.backdrop {
		position: absolute;
		inset: 0;
		border: none;
		padding: 0;
		background: rgba(11, 18, 21, 0.45);
		cursor: default;
	}

	.panel {
		position: relative;
		width: min(40rem, 100%);
		max-height: min(85svh, calc(100svh - 1.5rem));
		display: flex;
		flex-direction: column;
		background: var(--dk-surface);
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 20%, transparent);
		border-radius: 0.85rem;
		box-shadow: 0 18px 50px rgba(11, 18, 21, 0.3);
		overflow: hidden;
	}

	header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		border-bottom: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
	}

	h2 {
		margin: 0;
		font-size: 1.05rem;
	}

	h3 {
		font-size: 0.85rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		opacity: 0.55;
		margin: 1rem 0 0.5rem;
	}

	header button {
		font: inherit;
		font-size: 0.85rem;
		font-weight: 600;
		padding: 0.3rem 0.7rem;
		border: 1.5px solid var(--dk-ink);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	.error {
		margin: 0;
		padding: 0.5rem 1rem;
		color: var(--dk-danger);
		font-size: 0.85rem;
	}

	.browse,
	.configure {
		padding: 1rem;
		overflow-y: auto;
	}

	.subtle {
		opacity: 0.65;
		font-size: 0.9rem;
	}

	.upload {
		font: inherit;
		font-weight: 600;
		padding: 0.55rem 1rem;
		border: 1.5px dashed color-mix(in srgb, var(--dk-ink) 40%, transparent);
		border-radius: 0.6rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
		width: 100%;
	}

	.grid {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(8rem, 1fr));
		gap: 0.75rem;
	}

	.grid button {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		width: 100%;
		padding: 0.3rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 12%, transparent);
		border-radius: 0.5rem;
		background: transparent;
		color: inherit;
		cursor: pointer;
	}

	.grid img {
		width: 100%;
		aspect-ratio: 4 / 3;
		object-fit: cover;
		border-radius: 0.35rem;
		background: color-mix(in srgb, var(--dk-ink) 6%, transparent);
	}

	.grid span {
		font-size: 0.72rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.preview {
		max-width: 100%;
		max-height: 12rem;
		border-radius: 0.5rem;
		display: block;
		margin: 0 auto 0.75rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		font-weight: 600;
		margin-bottom: 0.75rem;
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

	.modes,
	.opts {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		margin-bottom: 0.75rem;
	}

	.modes button,
	.opts button {
		font: inherit;
		font-size: 0.82rem;
		font-weight: 600;
		padding: 0.35rem 0.7rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 25%, transparent);
		border-radius: 0.5rem;
		background: transparent;
		color: var(--dk-ink);
		cursor: pointer;
	}

	.modes button.active,
	.opts button.active {
		background: var(--dk-ink);
		color: var(--dk-bg);
		border-color: var(--dk-ink);
	}

	.footer {
		display: flex;
		justify-content: flex-end;
		gap: 0.5rem;
	}

	.footer button {
		font: inherit;
		font-weight: 600;
		padding: 0.45rem 0.9rem;
		border-radius: 0.5rem;
		cursor: pointer;
	}

	.ghost {
		background: transparent;
		border: 1.5px solid var(--dk-ink);
		color: var(--dk-ink);
	}

	.primary {
		background: var(--dk-ink);
		border: 1.5px solid var(--dk-ink);
		color: var(--dk-bg);
	}
</style>
