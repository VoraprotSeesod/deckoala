import { error, redirect } from '@sveltejs/kit';
import { api, ApiError } from '$lib/api';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, parent }) => {
	await parent();
	try {
		return { deck: await api.decks.get(params.id) };
	} catch (e) {
		if (e instanceof ApiError && e.status === 401) {
			// Cached layout auth + expired session — go to sign-in.
			redirect(307, '/login');
		}
		if (e instanceof ApiError && e.status === 404) {
			error(404, 'Deck not found');
		}
		throw e;
	}
};
