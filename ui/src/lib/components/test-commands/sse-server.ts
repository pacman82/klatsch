import type { BrowserCommand } from 'vitest/node';
import http from 'http';
import type { AddressInfo } from 'net';

let server: http.Server | null = null;
let clients: http.ServerResponse[] = [];
let clientConnected: (() => void) | null = null;

export const startSseServer: BrowserCommand<[]> = async () => {
	return new Promise<number>((resolve) => {
		server = http.createServer((_req, res) => {
			res.writeHead(200, {
				'Content-Type': 'text/event-stream',
				'Cache-Control': 'no-cache',
				'Connection': 'keep-alive',
				'Access-Control-Allow-Origin': '*',
			});
			clients.push(res);
			if (clientConnected) {
				clientConnected();
				clientConnected = null;
			}
		});
		server.listen(0, () => {
			resolve((server!.address() as AddressInfo).port);
		});
	});
};

export const sendSseEvent: BrowserCommand<[data: string]> = async (_ctx, data) => {
	for (const client of clients) {
		client.write(`data: ${data}\n\n`);
	}
};

export const waitForSseClient: BrowserCommand<[]> = async () => {
	if (clients.length > 0) return;
	return new Promise<void>((resolve) => {
		clientConnected = resolve;
	});
};

export const endSseStream: BrowserCommand<[]> = async () => {
	for (const client of clients) {
		client.end();
	}
	clients = [];
};

export const stopSseServer: BrowserCommand<[]> = async () => {
	for (const client of clients) {
		client.end();
	}
	clients = [];
	await new Promise<void>((resolve) => server?.close(() => resolve()));
	server = null;
};
