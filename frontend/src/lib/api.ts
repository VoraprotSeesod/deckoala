export type User = {
	id: string;
	username: string;
	isAdmin: boolean;
	createdAt: string;
};

export type Instance = {
	allowSignup: boolean;
	hasUsers: boolean;
};

export class ApiError extends Error {
	status: number;
	constructor(status: number, message: string) {
		super(message);
		this.status = status;
	}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	const response = await fetch(path, {
		...init,
		headers: { 'content-type': 'application/json', ...init?.headers }
	});
	if (!response.ok) {
		let message = response.statusText;
		try {
			const body = await response.json();
			if (typeof body?.error === 'string') message = body.error;
		} catch {
			// non-JSON error body; keep statusText
		}
		throw new ApiError(response.status, message);
	}
	if (response.status === 204) return undefined as T;
	return response.json() as Promise<T>;
}

export type DeckMeta = {
	id: string;
	title: string;
	theme: string;
	createdAt: string;
	updatedAt: string;
};

export type Deck = DeckMeta & { markdown: string };

export type RevisionMeta = {
	id: string;
	createdAt: string;
	sizeBytes: number;
};

export type Revision = {
	id: string;
	createdAt: string;
	markdown: string;
};

export const api = {
	instance: () => request<Instance>('/api/instance'),
	me: () => request<User>('/api/auth/me'),
	decks: {
		list: () => request<DeckMeta[]>('/api/decks'),
		create: (body: { title?: string; markdown?: string } = {}) =>
			request<Deck>('/api/decks', { method: 'POST', body: JSON.stringify(body) }),
		get: (id: string) => request<Deck>(`/api/decks/${id}`),
		update: (id: string, body: { title?: string; markdown?: string; baseUpdatedAt?: string }) =>
			request<Deck>(`/api/decks/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
		remove: (id: string) => request<void>(`/api/decks/${id}`, { method: 'DELETE' }),
		duplicate: (id: string) => request<Deck>(`/api/decks/${id}/duplicate`, { method: 'POST' }),
		exportUrl: (id: string) => `/api/decks/${id}/export`
	},
	revisions: {
		list: (deckId: string) => request<RevisionMeta[]>(`/api/decks/${deckId}/revisions`),
		get: (deckId: string, revId: string) =>
			request<Revision>(`/api/decks/${deckId}/revisions/${revId}`),
		restore: (deckId: string, revId: string) =>
			request<Deck>(`/api/decks/${deckId}/revisions/${revId}/restore`, { method: 'POST' })
	},
	register: (username: string, password: string) =>
		request<User>('/api/auth/register', {
			method: 'POST',
			body: JSON.stringify({ username, password })
		}),
	login: (username: string, password: string) =>
		request<User>('/api/auth/login', {
			method: 'POST',
			body: JSON.stringify({ username, password })
		}),
	logout: () => request<void>('/api/auth/logout', { method: 'POST' })
};
