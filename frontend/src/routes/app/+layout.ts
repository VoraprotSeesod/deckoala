import { redirect } from '@sveltejs/kit';
import { api, ApiError } from '$lib/api';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async () => {
	try {
		return { user: await api.me() };
	} catch (e) {
		if (e instanceof ApiError && e.status === 401) {
			redirect(307, '/login');
		}
		throw e;
	}
};
