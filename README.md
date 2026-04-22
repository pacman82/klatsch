# Klatsch


A self hosted chat server which is painless to operate. Main motivation for this project is for me to dabble gain some experience with Svelte. To learn about writing complete web applications which ship as a single self contained binary.

work in progress

- [x] Web Frontend
- [x] Send and Receive messages
- [x] Persistence: Remember conversation state after reboot.
- [ ] Authentication


## Installation

### Binary release

Check the Github release for prebuild binaries. Klatsch is self contained, just unpack the archive and run the executable.

### Docker

You can also run it in a docker container.

```shell
docker run -d --name klatsch \
  -p 3000:3000 \
  # Conversation is stored in klatsch-data on host
  -v klatsch-data:/data \
  ghcr.io/pacman82/klatsch:latest
```

### Building from source

Check out this repository. With `npm` and `cargo` installed run. You finde the executable in the `target/release` subfolder.

```shell
cargo build --release
```

## Operation

I am assuming klatsch has zero production Users. If you intend to use klatsch for anything, please let me know in an issue. I would than start versioned releases and provide migrations for persistence.

Klatsch has backend, frontend and persistence all in one binary. Klatsch boots with sensible options by default. You can configure it using environment variables or by providing a `.env` file. You can look at `.env.example` to learn what options are available.

### Logging

Klatsch logs to standard error. The log level can be controlled via the `LOG_LEVEL` environment variable. It can be set to ERROR, WARN, INFO, DEBUG and TRACE. INFO is the default. You can set separate log levels for individial targets. Special instructions override global ones. E.g. "warn,server=info". The log targets are:

- **`app`**
- **`server`**
- **`http`**
- **`persistence`**

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

### Local execution

To run the klatsch server locally for development I recommend to make a local copy of `.env.example` as `.env` which is ignored by git.

```shell
cp .env.example .env
```

Klatsch will boot fine with the default options, but binding to `127.0.0.1` is more secure for development then you do not expect any external traffic anyway.

### UI development server

Start the klatsch server with:

```shell
cargo run
```

This is both backend and frontend. However the integrated fronted will not hot reload. To shorten the iteration cycle for UI work you can start a dev server for hot reaload by navigating to the `ui` and using:

```shell
npm run dev
```

Enabling Sabotage mode for testing error handling in the frontend

```shell
curl -X PUT http://localhost:3000/sabotage -H 'Content-Type: application/json' -d 'true'
```

## Attribution

Coffee icon is downloaded from <https://icon-icons.com/icon/coffee/63177> and is licensed under the [Creative Commons Attribution 4.0 International (CC BY 4.0)](https://creativecommons.org/licenses/by/4.0/) license.
