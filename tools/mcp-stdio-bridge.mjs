#!/usr/bin/env node
// Deckoala MCP stdio bridge (BRIEF-0011).
//
// Deckoala speaks MCP over plain HTTP JSON-RPC at POST /mcp with a Bearer
// token. Clients that only support stdio transport (e.g. Claude Desktop) can
// use this shim: it pipes newline-delimited JSON-RPC from stdin to that
// endpoint and writes each reply back to stdout.
//
//   DECKOALA_MCP_URL=https://deck.example.com/mcp \
//   DECKOALA_MCP_TOKEN=dko_… node mcp-stdio-bridge.mjs
//
// Zero dependencies; needs Node 18+ (global fetch). Diagnostics go to stderr
// only — anything on stdout must be valid JSON-RPC or the client will choke.

const url = process.env.DECKOALA_MCP_URL;
const token = process.env.DECKOALA_MCP_TOKEN;

if (!url || !token) {
	process.stderr.write('DECKOALA_MCP_URL and DECKOALA_MCP_TOKEN must both be set\n');
	process.exit(1);
}

/** JSON-RPC error we can hand back when the transport itself fails, so the
 *  client sees a real error instead of hanging on a missing reply. */
function transportError(id, message) {
	return JSON.stringify({ jsonrpc: '2.0', id, error: { code: -32603, message } });
}

async function forward(line) {
	let id = null;
	try {
		id = JSON.parse(line)?.id ?? null;
	} catch {
		// Let the server produce the -32700; it owns the protocol.
	}

	let response;
	try {
		response = await fetch(url, {
			method: 'POST',
			headers: {
				'content-type': 'application/json',
				authorization: `Bearer ${token}`
			},
			body: line
		});
	} catch (e) {
		return id === null ? null : transportError(id, `cannot reach Deckoala: ${e.message}`);
	}

	const body = (await response.text()).trim();
	if (!response.ok) {
		process.stderr.write(`deckoala: HTTP ${response.status} ${body.slice(0, 200)}\n`);
		return id === null ? null : transportError(id, `Deckoala returned HTTP ${response.status}`);
	}
	// 202 + empty body is the correct answer to a notification: stay silent.
	return body === '' ? null : body;
}

// Requests are forwarded in arrival order; JSON-RPC ids let the client match
// replies, but keeping order avoids surprising clients that assume it.
let queue = Promise.resolve();
let buffer = '';

process.stdin.setEncoding('utf8');
process.stdin.on('data', (chunk) => {
	buffer += chunk;
	let newline;
	while ((newline = buffer.indexOf('\n')) !== -1) {
		const line = buffer.slice(0, newline).trim();
		buffer = buffer.slice(newline + 1);
		if (!line) continue;
		queue = queue.then(async () => {
			const reply = await forward(line);
			if (reply !== null) process.stdout.write(reply + '\n');
		});
	}
});

// No explicit exit on stdin end: letting the event loop drain naturally
// flushes stdout and avoids a libuv teardown assertion on Windows.

// The client may close the pipe first; that is a normal shutdown, not a crash.
process.stdout.on('error', (e) => {
	if (e.code !== 'EPIPE') process.stderr.write(`deckoala: stdout ${e.message}\n`);
});
