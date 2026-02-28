# Klatsch

work in progress

A self hosted chat server which is painless to operate. Main motivation for this project is for me to dabble gain some experience with Svelte. To learn about writing complet web applications which ship as a single self contained binary.

## Installation

Klatsch is not released yet. You can build it from this source executing:

```shell
cargo build --release
```

## Operation

Klatsch has backend, frontend and persistence all in one binary. In order for it to persist the Chat history the `DATABASE_PATH` environment variable needs to be set, or sepcified in a `.env` file.

## Development

### Prerequisites

Klatsch is written in Rust and Svelte. To build and test it you need a rust toolchain and npm installed.

* Rust toolchain: <https://rustup.rs/>
* Node.js and npm: <https://nodejs.org/en/download/>

### Tests

Integration and unit tests for the Backend can be run with:

```shell
cargo test
```

To run the tests for the frontend navigate to the `ui` subdirectory and run:

```shell
npm test
```

### UI development server

Start the klatsch server with:

```shell
cargo run
```

This is both backend and frontend. However the integrated fronted will not hot reload. To shorten the iteration cycle for UI work you can start a dev server for hot reaload by navigating to the `ui` and using:

```shell
npm run dev
```

## Attribution

Coffee icon is downloaded from <https://icon-icons.com/icon/coffee/63177> and is licensed under the [Creative Commons Attribution 4.0 International (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/) license.
