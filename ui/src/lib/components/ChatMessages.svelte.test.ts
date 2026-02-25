import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import ChatMessages from './ChatMessages.svelte';

class EventSourcePuppet {
	static last: EventSourcePuppet;
	onmessage: ((event: MessageEvent) => void) | null = null;
	onopen: ((event: Event) => void) | null = null;
	onerror: ((event: Event) => void) | null = null;
	close = vi.fn();
	constructor() { EventSourcePuppet.last = this; }
}

test('error clears when connection is reestablished', async () => {
	// Given a connection that has errored
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;
	puppet.onerror!(new Event('test error'));

	// When the connection is reestablished
	puppet.onopen!(new Event('open'));

	// Then the error disappears
	await expect.element(screen.getByText('Reconnecting...')).not.toBeInTheDocument();
});

test('connection error is displayed to the user', async () => {
	// Given a connection that fails
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When the connection errors
	puppet.onerror!(new Event('test error'));

	// Then the error is displayed
	await expect.element(screen.getByText('Reconnecting...')).toBeVisible();
});
