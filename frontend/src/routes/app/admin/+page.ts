import { api, type AdminSettings } from '$lib/api';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
	const { user } = await parent(); // from the /app layout auth check
	// Non-admins never call the admin API at all (it would 403 anyway) — the
	// page renders an "admin only" state instead.
	const settings: AdminSettings | null = user.isAdmin ? await api.admin.getSettings() : null;
	return { user, settings };
};
