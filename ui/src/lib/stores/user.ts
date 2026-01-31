import { writable } from 'svelte/store';

const STORAGE_KEY = 'klatsch:user';

function defaultName() {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return `user-${crypto.randomUUID().slice(0, 8)}`;
  }
  return `user-${Math.floor(Math.random() * 10000)}`;
}

function createUserStore() {
  let initial: string;
  try {
    initial = (typeof window !== 'undefined' && window.localStorage && window.localStorage.getItem(STORAGE_KEY)) || defaultName();
  } catch (_) {
    initial = defaultName();
  }
  const { subscribe, set, update } = writable<string>(initial);

  return {
    subscribe,
    set(value: string) {
      try {
        if (typeof window !== 'undefined' && window.localStorage) {
          window.localStorage.setItem(STORAGE_KEY, value);
        }
      } catch (_) {}
      set(value);
    },
    update,
  };
}

export const user = createUserStore();
