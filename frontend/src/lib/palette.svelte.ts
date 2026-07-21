/** Shared handle for the command palette (BRIEF-0009b).
 *
 * `/app/+layout.svelte` owns the overlays; pages contribute their own commands
 * through this handle rather than having the layout reconstruct page-local
 * state (the dashboard's hidden file input, the editor's `saveNow`) it cannot
 * reach. Passed down via Svelte context, so `commands.ts` stays pure.
 */
import { getContext, setContext } from 'svelte';
import type { Command } from './commands';

const KEY = Symbol('deckoala.palette');

export type PaletteHandle = {
	/** Publish this page's commands; call the returned function to withdraw
	 * them (an `$effect` cleanup does this on navigation). */
	register: (commands: Command[]) => () => void;
	open: () => void;
	/** Drop the cached deck list after a mutation, so a jump can't hit a deck
	 * that no longer exists. */
	invalidateDecks: () => void;
};

export function setPalette(handle: PaletteHandle) {
	setContext(KEY, handle);
}

/** Pages outside the /app layout (none today) get a no-op handle rather than
 * a crash, so a component can be reused without knowing who mounts it. */
export function getPalette(): PaletteHandle {
	return (
		getContext<PaletteHandle | undefined>(KEY) ?? {
			register: () => () => {},
			open: () => {},
			invalidateDecks: () => {}
		}
	);
}
