import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import ChatMessages from './ChatMessages.svelte';

class EventSourcePuppet {
	static last: EventSourcePuppet;
	onmessage: ((event: MessageEvent) => void) | null = null;
	onopen: ((event: Event) => void) | null = null;
	onerror: ((event: Event) => void) | null = null;
	close = vi.fn();
	readyState: number = EventSource.OPEN;
	constructor() {
		EventSourcePuppet.last = this;
	}
}

test('connection error clears when connection is reestablished', async () => {
	// Given a connection that has errored
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;
	puppet.onerror!(new Event('error'));

	// When the connection is reestablished
	puppet.onopen!(new Event('open'));

	// Then the error disappears
	await expect
		.element(screen.getByText('Error connecting to server. Reconnecting...'))
		.not.toBeInTheDocument();
});

test('connection error shows reconnecting message', async () => {
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When the connection errors
	puppet.onerror!(new Event('error'));

	// Then a connection error message is displayed
	await expect
		.element(screen.getByText('Error connecting to server. Reconnecting...'))
		.toBeVisible();
});

test('server error does not persist across reconnections', async () => {
	// Given a server error that was resolved by reconnecting
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;
	puppet.onerror!(new MessageEvent('error', { data: 'Sabotage' }));
	puppet.onerror!(new Event('error'));
	puppet.onopen!(new Event('open'));

	// When a plain connection error occurs
	puppet.onerror!(new Event('error'));

	// Then the generic connection error is shown, not the stale server error
	await expect
		.element(screen.getByText('Error connecting to server. Reconnecting...'))
		.toBeVisible();
});

test('receives messages after server restart', async () => {
	vi.stubGlobal('EventSource', EventSourcePuppet);
	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When the server shuts down cleanly
	puppet.readyState = EventSource.CLOSED;
	puppet.onerror!(new Event('error'));

	// Then new messages are received after the server comes back
	const reconnected = EventSourcePuppet.last;
	expect(reconnected).not.toBe(puppet);
	reconnected.onmessage!(new MessageEvent('message', {
		data: JSON.stringify({ id: '1', sender: 'alice', content: 'hello', timestamp_ms: 0 })
	}));

	await expect.element(screen.getByText('hello')).toBeVisible();
});

test('server error shows error message from server', async () => {
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When a server error arrives followed by a connection drop
	puppet.onerror!(new MessageEvent('error', { data: 'Sabotage' }));
	puppet.onerror!(new Event('error'));

	// Then the server error message is displayed, not the generic connection error
	await expect.element(screen.getByText('Server error: "Sabotage". Reconnecting...')).toBeVisible();
});
