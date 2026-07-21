<script lang="ts">
	import { settings, toggleLocale, toggleTheme, t } from '$lib/i18n.svelte';
</script>

<div class="settings">
	<button
		class="tog"
		onclick={toggleLocale}
		title={t('settings.language')}
		aria-label={t('settings.language')}
	>
		{settings.locale === 'th' ? 'EN' : 'ไทย'}
	</button>
	<button
		class="tog icon"
		onclick={toggleTheme}
		title={settings.theme === 'dark' ? t('settings.toLight') : t('settings.toDark')}
		aria-label={settings.theme === 'dark' ? t('settings.toLight') : t('settings.toDark')}
	>
		{settings.theme === 'dark' ? '☀' : '🌙'}
	</button>
</div>

<style>
	.settings {
		display: inline-flex;
		gap: 0.35rem;
	}

	.tog {
		font: inherit;
		font-size: 0.8rem;
		font-weight: 700;
		min-width: 2.1rem;
		padding: 0.3rem 0.5rem;
		border: 1.5px solid color-mix(in srgb, var(--dk-ink) 30%, transparent);
		border-radius: 0.5rem;
		background: var(--dk-surface);
		color: var(--dk-ink);
		cursor: pointer;
		line-height: 1;
	}

	.tog.icon {
		font-size: 1rem;
	}

	.tog:hover {
		border-color: var(--dk-ink);
	}
</style>
