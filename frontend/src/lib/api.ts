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

export const api = {
	instance: () => request<Instance>('/api/instance'),
	me: () => request<User>('/api/auth/me'),
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
