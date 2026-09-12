import { render } from 'vitest-browser-svelte';
import { expect, test, vi } from 'vitest';
import { user } from '$lib/user.svelte';
import { user_cache } from '$lib/user_cache.svelte';

// Whether `element` is actually within the area of `container` the user can currently see,
// as opposed to merely present in the DOM but scrolled out of view.
function isVisibleWithin(element: HTMLElement, container: HTMLElement): boolean {
	const elementRect = element.getBoundingClientRect();
	const containerRect = container.getBoundingClientRect();
	return elementRect.top >= containerRect.top && elementRect.bottom <= containerRect.bottom;
}

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

import ChatMessages from './ChatMessages.svelte';

test('my messages are displayed on the right, others on the left', async () => {
	// Given Alice is logged in
	const ALICE_ID = 'ab70b6ca-4139-499f-a66d-15e88f081fb1';
	const BOB_ID = 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb';
	user.login(ALICE_ID);
	vi.stubGlobal('EventSource', EventSourcePuppet);
	vi.spyOn(user_cache, 'resolve').mockImplementation((id) => {
		if (id === ALICE_ID) return { name: 'Alice' };
		if (id === BOB_ID) return { name: 'Bob' };
	});

	const screen = await render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When Messages of Alice and Bob are received
	puppet.onmessage!(
		new MessageEvent('message', {
			data: JSON.stringify({
				id: '1',
				sender: 'Alice',
				sender_id: ALICE_ID,
				content: 'mine',
				timestamp_ms: 0
			})
		})
	);
	puppet.onmessage!(
		new MessageEvent('message', {
			data: JSON.stringify({
				id: '2',
				sender: 'Bob',
				sender_id: BOB_ID,
				content: 'theirs',
				timestamp_ms: 0
			})
		})
	);

	// Then Alice's messages are displayed on the right (csv class 'me'), Bob's on the left (csv class 'them')

	// Wait for elements to be rendered
	await expect.element(screen.getByText('mine')).toBeVisible();
	await expect.element(screen.getByText('theirs')).toBeVisible();
	expect(screen.getByText('mine').query()?.closest('.message-row')?.classList.contains('me')).toBe(
		true
	);
	expect(
		screen.getByText('theirs').query()?.closest('.message-row')?.classList.contains('them')
	).toBe(true);
});

test('connection error clears when connection is reestablished', async () => {
	// Given a connection that has errored
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = await render(ChatMessages);
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

	const screen = await render(ChatMessages);
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

	const screen = await render(ChatMessages);
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
	const ALICE_ID = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
	vi.stubGlobal('EventSource', EventSourcePuppet);
	vi.stubGlobal(
		'fetch',
		vi.fn().mockResolvedValue(new Response(JSON.stringify({ name: 'Alice' }), { status: 200 }))
	);
	const screen = await render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When the server shuts down cleanly
	puppet.readyState = EventSource.CLOSED;
	puppet.onerror!(new Event('error'));

	// Then new messages are received after the server comes back
	const reconnected = EventSourcePuppet.last;
	expect(reconnected).not.toBe(puppet);
	reconnected.onmessage!(
		new MessageEvent('message', {
			data: JSON.stringify({ id: '1', sender_id: ALICE_ID, content: 'hello', timestamp_ms: 0 })
		})
	);

	await expect.element(screen.getByText('hello')).toBeVisible();
});

test('the newest message becomes visible without the user having to scroll', async () => {
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = await render(ChatMessages);
	const puppet = EventSourcePuppet.last;
	const container = screen.container.querySelector('.chat-container') as HTMLElement;
	// The component isn't inside the page's flex shell here, so it won't get a real height from
	// its own CSS. Give it one so overflow — and therefore visibility — is genuine, not assumed.
	container.style.height = '150px';

	// Given enough history that it overflows the visible area
	for (let i = 0; i < 20; i++) {
		puppet.onmessage!(
			new MessageEvent('message', {
				data: JSON.stringify({ id: `h${i}`, sender_id: 'x', content: `history ${i}`, timestamp_ms: 0 })
			})
		);
	}
	await expect.element(screen.getByText('history 19')).toBeVisible();

	// When a new message arrives while the user is already caught up
	puppet.onmessage!(
		new MessageEvent('message', {
			data: JSON.stringify({ id: 'new', sender_id: 'x', content: 'brand new', timestamp_ms: 0 })
		})
	);
	await expect.element(screen.getByText('brand new')).toBeVisible();

	// Then the user can see it without scrolling themselves
	const newMessage = screen.getByText('brand new').query() as HTMLElement;
	expect(isVisibleWithin(newMessage, container)).toBe(true);
});

test('does not disturb what the user is reading when a new message arrives', async () => {
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = await render(ChatMessages);
	const puppet = EventSourcePuppet.last;
	const container = screen.container.querySelector('.chat-container') as HTMLElement;
	container.style.height = '150px';

	// Given enough history that it overflows the visible area
	for (let i = 0; i < 20; i++) {
		puppet.onmessage!(
			new MessageEvent('message', {
				data: JSON.stringify({ id: `h${i}`, sender_id: 'x', content: `history ${i}`, timestamp_ms: 0 })
			})
		);
	}
	await expect.element(screen.getByText('history 19')).toBeVisible();

	// And the user has scrolled up to read earlier history
	container.scrollTop = 0;
	container.dispatchEvent(new Event('scroll'));
	await expect.element(screen.getByText('history 0')).toBeVisible();
	const messageBeingRead = screen.getByText('history 0').query() as HTMLElement;
	expect(isVisibleWithin(messageBeingRead, container)).toBe(true);

	// When a new message arrives
	puppet.onmessage!(
		new MessageEvent('message', {
			data: JSON.stringify({ id: 'new', sender_id: 'x', content: 'brand new', timestamp_ms: 0 })
		})
	);
	await expect.element(screen.getByText('brand new')).toBeInTheDocument();

	// Then what the user was reading is still visible — they were not pulled back to the bottom
	expect(isVisibleWithin(messageBeingRead, container)).toBe(true);
});

test('server error shows error message from server', async () => {
	vi.stubGlobal('EventSource', EventSourcePuppet);

	const screen = await render(ChatMessages);
	const puppet = EventSourcePuppet.last;

	// When a server error arrives followed by a connection drop
	puppet.onerror!(new MessageEvent('error', { data: 'Sabotage' }));
	puppet.onerror!(new Event('error'));

	// Then the server error message is displayed, not the generic connection error
	await expect.element(screen.getByText('Server error: "Sabotage". Reconnecting...')).toBeVisible();
});
