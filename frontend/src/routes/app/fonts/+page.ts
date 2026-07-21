import { api } from '$lib/api';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
	const { user } = await parent(); // from the /app layout auth check
	return { user, fonts: await api.fonts.list() };
};
