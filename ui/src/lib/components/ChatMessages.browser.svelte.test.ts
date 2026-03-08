import { render } from 'vitest-browser-svelte';
import { commands } from 'vitest/browser';
import { afterEach, expect, test } from 'vitest';
import ChatMessages from './ChatMessages.svelte';

declare module 'vitest/browser' {
	interface BrowserCommands {
		startSseServer: () => Promise<number>;
		sendSseEvent: (data: string) => Promise<void>;
		waitForSseClient: () => Promise<void>;
		endSseStream: () => Promise<void>;
		stopSseServer: () => Promise<void>;
	}
}

afterEach(async () => {
	await commands.stopSseServer();
});

test('receives messages after server restart', async () => {
	const port = await commands.startSseServer();
	const screen = render(ChatMessages, {
		props: { eventsUrl: `http://localhost:${port}/events` }
	});

	// When the server ends the stream cleanly (graceful shutdown)
	await commands.endSseStream();

	// And the server comes back with a new message
	await commands.waitForSseClient();
	await commands.sendSseEvent(
		JSON.stringify({ id: '1', sender: 'alice', content: 'hello', timestamp_ms: 0 })
	);

	// Then the message is received
	await expect.element(screen.getByText('hello')).toBeVisible();
});
