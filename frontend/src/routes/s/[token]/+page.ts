import { error } from '@sveltejs/kit';
import { api, ApiError } from '$lib/api';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params }) => {
	try {
		// The token IS the authorization — no session/account needed. Loading it
		// also sets the per-deck share cookie so the deck's images can load.
		return { token: params.token, deck: await api.shared.get(params.token) };
	} catch (e) {
		if (e instanceof ApiError && e.status === 404) {
			error(404, 'This share link is invalid or no longer active.');
		}
		throw e;
	}
};
