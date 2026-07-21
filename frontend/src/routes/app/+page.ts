import { redirect } from '@sveltejs/kit';
import { api, ApiError } from '$lib/api';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
	// Wait for the layout's auth check so a 401 redirects instead of erroring.
	await parent();
	try {
		return { decks: await api.decks.list() };
	} catch (e) {
		// The layout's auth result is cached on client-side navigation, so an
		// expired session surfaces here — send the user to sign-in, not to a
		// generic error page.
		if (e instanceof ApiError && e.status === 401) {
			redirect(307, '/login');
		}
		throw e;
	}
};
