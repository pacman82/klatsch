// This is required in order to make the static adapter work. We want to prerender all pages in
// order to host the UI statically and only require one binary and serving the UI directly from the
// chat server.
export const prerender = true;
