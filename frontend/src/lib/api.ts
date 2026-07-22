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

/** `/api/auth/me` adds two session-gated flags on top of `User`. They are
 * deliberately NOT on the anonymous `/api/instance`. */
export type Me = User & {
	rootPasswordIsDefault: boolean;
	/** The only AI signal a non-admin gets — provider/baseUrl/model are admin-only. */
	aiEnabled: boolean;
};

export type AiProvider = 'anthropic' | 'openai';

export type AiSettings = {
	enabled: boolean;
	provider: AiProvider;
	baseUrl: string;
	model: string;
	/** The key itself is never sent by the server. */
	apiKeySet: boolean;
	apiKeyLast4: string | null;
};

export type AdminSettings = {
	ai: AiSettings;
	rootPasswordIsDefault: boolean;
};

export type AiSettingsUpdate = {
	enabled: boolean;
	provider: AiProvider;
	baseUrl: string;
	model: string;
	/** Omit/empty to keep the stored key. */
	apiKey?: string;
	removeApiKey?: boolean;
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

export type UploadedAsset = {
	id: string;
	url: string;
	originalName: string;
	mime: string;
	sizeBytes: number;
};

export type DeckAsset = UploadedAsset & { createdAt: string };

export type FontInfo = {
	id: string;
	family: string;
	weight: string;
	style: string;
	format: string;
	source: string;
	createdAt: string;
};

export type ApiToken = {
	id: string;
	name: string;
	createdAt: string;
	lastUsedAt: string | null;
	revokedAt: string | null;
};

/** Only the create call ever carries `token` — it is never listed again. */
export type CreatedApiToken = {
	id: string;
	name: string;
	createdAt: string;
	token: string;
};

export type SharePermission = 'view' | 'edit';

export type ShareLink = {
	id: string;
	token: string;
	permission: SharePermission;
	url: string;
	createdAt: string;
	expiresAt: string | null;
	revokedAt: string | null;
	status: 'active' | 'revoked' | 'expired';
};

export type SharedDeck = Deck & { permission: SharePermission };

/** The data source a `<DeckEditor>` talks to — either the owner endpoints
 * (keyed by deck id) or the share-token endpoints. Lets one editor component
 * serve both /app/deck/[id] and /s/[token] without knowing which it is. */
export type EditorAdapter = {
	update(body: { title?: string; markdown?: string; baseUpdatedAt?: string }): Promise<Deck>;
	listRevisions(): Promise<RevisionMeta[]>;
	getRevision(revId: string): Promise<Revision>;
	restoreRevision(revId: string): Promise<Deck>;
	uploadAsset(file: File): Promise<UploadedAsset>;
	listAssets(): Promise<DeckAsset[]>;
	downloadPdf(title: string): Promise<void>;
	exportMdUrl: string;
};

/** Download a PDF blob from a POST endpoint (shared by owner + share-token). */
async function downloadPdfFrom(url: string, fallbackTitle: string): Promise<void> {
	const res = await fetch(url, { method: 'POST' });
	if (!res.ok) {
		let message = res.statusText;
		try {
			const body = await res.json();
			if (typeof body?.error === 'string') message = body.error;
		} catch {
			// keep statusText
		}
		throw new ApiError(res.status, message);
	}
	const blob = await res.blob();
	const disposition = res.headers.get('content-disposition') ?? '';
	const match = disposition.match(/filename="([^"]+)"/);
	const filename = match ? match[1] : `${fallbackTitle || 'deck'}.pdf`;
	const objectUrl = URL.createObjectURL(blob);
	const a = document.createElement('a');
	a.href = objectUrl;
	a.download = filename;
	document.body.appendChild(a);
	a.click();
	a.remove();
	URL.revokeObjectURL(objectUrl);
}

/** POST a single file as multipart/form-data and return the parsed JSON. */
async function uploadFileTo<T>(url: string, file: File): Promise<T> {
	const form = new FormData();
	form.append('file', file);
	const res = await fetch(url, { method: 'POST', body: form });
	if (!res.ok) {
		let message = res.statusText;
		try {
			const body = await res.json();
			if (typeof body?.error === 'string') message = body.error;
		} catch {
			// keep statusText
		}
		throw new ApiError(res.status, message);
	}
	return res.json() as Promise<T>;
}

export const api = {
	instance: () => request<Instance>('/api/instance'),
	me: () => request<Me>('/api/auth/me'),
	changePassword: (currentPassword: string, newPassword: string) =>
		request<void>('/api/auth/password', {
			method: 'POST',
			body: JSON.stringify({ currentPassword, newPassword })
		}),
	admin: {
		getSettings: () => request<AdminSettings>('/api/admin/settings'),
		putSettings: (body: AiSettingsUpdate) =>
			request<AdminSettings>('/api/admin/settings', { method: 'PUT', body: JSON.stringify(body) })
	},
	ai: {
		generate: (prompt: string, existingMarkdown?: string) =>
			request<{ markdown: string }>('/api/ai/generate', {
				method: 'POST',
				body: JSON.stringify({ prompt, existingMarkdown })
			})
	},
	tokens: {
		list: () => request<ApiToken[]>('/api/tokens'),
		create: (name: string) =>
			request<CreatedApiToken>('/api/tokens', { method: 'POST', body: JSON.stringify({ name }) }),
		revoke: (id: string) => request<void>(`/api/tokens/${id}`, { method: 'DELETE' })
	},
	decks: {
		list: () => request<DeckMeta[]>('/api/decks'),
		create: (body: { title?: string; markdown?: string } = {}) =>
			request<Deck>('/api/decks', { method: 'POST', body: JSON.stringify(body) }),
		get: (id: string) => request<Deck>(`/api/decks/${id}`),
		update: (id: string, body: { title?: string; markdown?: string; baseUpdatedAt?: string }) =>
			request<Deck>(`/api/decks/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
		remove: (id: string) => request<void>(`/api/decks/${id}`, { method: 'DELETE' }),
		duplicate: (id: string) => request<Deck>(`/api/decks/${id}/duplicate`, { method: 'POST' }),
		exportUrl: (id: string) => `/api/decks/${id}/export`,
		// Server generates the PDF (headless Chromium — slow); download the blob.
		downloadPdf: (id: string, fallbackTitle: string) =>
			downloadPdfFrom(`/api/decks/${id}/export/pdf`, fallbackTitle)
	},
	fonts: {
		list: () => request<FontInfo[]>('/api/fonts'),
		upload: async (
			family: string,
			weight: string,
			style: string,
			file: File
		): Promise<FontInfo[]> => {
			const form = new FormData();
			form.append('family', family);
			form.append('weight', weight);
			form.append('style', style);
			form.append('file', file);
			const res = await fetch('/api/fonts', { method: 'POST', body: form });
			if (!res.ok) {
				let message = res.statusText;
				try {
					const body = await res.json();
					if (typeof body?.error === 'string') message = body.error;
				} catch {
					// keep statusText
				}
				throw new ApiError(res.status, message);
			}
			return res.json() as Promise<FontInfo[]>;
		},
		google: (family: string, weights: string[]) =>
			request<FontInfo[]>('/api/fonts/google', {
				method: 'POST',
				body: JSON.stringify({ family, weights })
			}),
		remove: (id: string) => request<void>(`/api/fonts/${id}`, { method: 'DELETE' }),
		// Re-fetch the document's fonts.css <link> so a just-installed font shows
		// without a full reload (fonts.css is served no-cache).
		reloadCss: () => {
			const link = document.querySelector<HTMLLinkElement>('link[data-deckoala-fonts]');
			if (link) link.href = `/api/fonts.css?t=${Date.now()}`;
		}
	},
	revisions: {
		list: (deckId: string) => request<RevisionMeta[]>(`/api/decks/${deckId}/revisions`),
		get: (deckId: string, revId: string) =>
			request<Revision>(`/api/decks/${deckId}/revisions/${revId}`),
		restore: (deckId: string, revId: string) =>
			request<Deck>(`/api/decks/${deckId}/revisions/${revId}/restore`, { method: 'POST' })
	},
	assets: {
		// FormData sets its own multipart Content-Type/boundary — do NOT pass
		// a JSON content-type header here.
		upload: (deckId: string, file: File) =>
			uploadFileTo<UploadedAsset>(`/api/decks/${deckId}/assets`, file),
		list: (deckId: string) => request<DeckAsset[]>(`/api/decks/${deckId}/assets`)
	},
	// Owner-side share-link management for a deck.
	shares: {
		list: (deckId: string) => request<ShareLink[]>(`/api/decks/${deckId}/shares`),
		create: (deckId: string, permission: SharePermission, expiresAt?: string | null) =>
			request<ShareLink>(`/api/decks/${deckId}/shares`, {
				method: 'POST',
				body: JSON.stringify({ permission, expiresAt: expiresAt ?? null })
			}),
		revoke: (deckId: string, shareId: string) =>
			request<void>(`/api/decks/${deckId}/shares/${shareId}`, { method: 'DELETE' })
	},
	// Token-scoped access for a share-link recipient (no account).
	shared: {
		get: (token: string) => request<SharedDeck>(`/api/s/${token}`),
		update: (token: string, body: { title?: string; markdown?: string; baseUpdatedAt?: string }) =>
			request<Deck>(`/api/s/${token}`, { method: 'PATCH', body: JSON.stringify(body) }),
		revisions: {
			list: (token: string) => request<RevisionMeta[]>(`/api/s/${token}/revisions`),
			get: (token: string, revId: string) =>
				request<Revision>(`/api/s/${token}/revisions/${revId}`),
			restore: (token: string, revId: string) =>
				request<Deck>(`/api/s/${token}/revisions/${revId}/restore`, { method: 'POST' })
		},
		uploadAsset: (token: string, file: File) =>
			uploadFileTo<UploadedAsset>(`/api/s/${token}/assets`, file),
		listAssets: (token: string) => request<DeckAsset[]>(`/api/s/${token}/assets`),
		exportMdUrl: (token: string) => `/api/s/${token}/export`,
		downloadPdf: (token: string, fallbackTitle: string) =>
			downloadPdfFrom(`/api/s/${token}/export/pdf`, fallbackTitle)
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

/** Editor adapter bound to a deck the caller owns (session-scoped endpoints). */
export function ownerAdapter(deckId: string): EditorAdapter {
	return {
		update: (body) => api.decks.update(deckId, body),
		listRevisions: () => api.revisions.list(deckId),
		getRevision: (revId) => api.revisions.get(deckId, revId),
		restoreRevision: (revId) => api.revisions.restore(deckId, revId),
		uploadAsset: (file) => api.assets.upload(deckId, file),
		listAssets: () => api.assets.list(deckId),
		downloadPdf: (title) => api.decks.downloadPdf(deckId, title),
		exportMdUrl: api.decks.exportUrl(deckId)
	};
}

/** Editor adapter bound to a share token (anonymous, token-scoped endpoints). */
export function sharedAdapter(token: string): EditorAdapter {
	return {
		update: (body) => api.shared.update(token, body),
		listRevisions: () => api.shared.revisions.list(token),
		getRevision: (revId) => api.shared.revisions.get(token, revId),
		restoreRevision: (revId) => api.shared.revisions.restore(token, revId),
		uploadAsset: (file) => api.shared.uploadAsset(token, file),
		listAssets: () => api.shared.listAssets(token),
		downloadPdf: (title) => api.shared.downloadPdf(token, title),
		exportMdUrl: api.shared.exportMdUrl(token)
	};
}
