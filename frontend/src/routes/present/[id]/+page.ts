import { error, redirect } from '@sveltejs/kit';
import { api, ApiError } from '$lib/api';
import type { PageLoad } from './$types';

// Present lives outside /app, so it does its own auth handling (no app-shell
// layout guard). It reads a snapshot of the deck; edits made afterwards in the
// editor are not reflected until the present window is reloaded.
export const load: PageLoad = async ({ params }) => {
	try {
		return { deck: await api.decks.get(params.id) };
	} catch (e) {
		if (e instanceof ApiError && e.status === 401) {
			redirect(307, '/login');
		}
		if (e instanceof ApiError && e.status === 404) {
			error(404, 'Deck not found');
		}
		throw e;
	}
};
