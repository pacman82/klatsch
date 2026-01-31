import { writable } from 'svelte/store';

// Simple in-memory reactive store for the current user name.
export const user = writable<string>('Bob');
