import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

type PrintDeck = { title: string; markdown: string; theme: string };

// The print view is loaded ONLY by the in-container headless Chromium, which
// carries the print-token cookie (not a session). Authorization is the cookie;
// a missing/invalid token yields 404.
export const load: PageLoad = async ({ params, fetch }) => {
	const res = await fetch(`/api/print/${params.id}`);
	if (!res.ok) {
		error(res.status === 404 ? 404 : 500, 'Print view not available');
	}
	return { deck: (await res.json()) as PrintDeck };
};
