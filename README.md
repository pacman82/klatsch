# Klatsch

work in progress

A self hosted chat server which is painless to operate

## Development

To develop the UI utilizing hot reloading you want to run the vite development server and the Rust backend in different processes.

Start the Rust backend with:

```shell
cargo run
```

Start the vite development server with:

```shell
cd ui
npm run dev
```

## Milestones

- [x] UI statically linked into server
- [x] Display messages
- [ ] Send messages as 'Bob'

## Attribution

Coffee icon is downloaded from <https://icon-icons.com/icon/coffee/63177> and is licensed under the [Creative Commons Attribution 4.0 International (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/) license.
