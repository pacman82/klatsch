# Changelog

`Klatsch` adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
## [0.2.0](https://github.com/pacman82/klatsch/compare/0.1.1...0.2.0) - 2026-10-11

### 🚀 Features

- [**breaking**] In order to prevent any random person to sign up, new users are now created via invite. For this purpose existing users can create an invite link. The signup option has been removed from the login page. If there are no users in the system, the signup page is shown instead of the login page and the first user can be created without an invite link.
- To pass messages via encryption support for Transport Layer Security (TLS) has been added. The supported TLS modes are: `off`, `proxy` and `static`. `off` deactivates TLS. `proxy` is the same, but klatsch is aware that communication is encrypted via a proxy (e.g. Caddy) and will set the secure attribute which prevent sending session cookies over unencrypted connections for added security. `static` terminates TLS in Klatsch itself (i.e. TLS encryption is performed by klatsch rather than a proxy). The TLS mode can be configured explicitly via environment variables. See `.env.example` for additional information about the environment variables.
- When a user is created a password may be provided to allow for authentication. Users can change their password in self service. This also enables existing users with no password to provide one.
- Once logged in, users are authenticated via sessions. Sessions have both a sliding and an absolute window for expiry. Both windows can be controlled via environment variables.
- *(ui)* Scrolling for ChatMessage component


### 🐛 Bug Fixes

- Signal handlers are now registered before emitting "ready". Previously there was a small time window for which terminating using Ctrl+C might not have worked after boot, even after "ready" had been emitted to the log output.


### 🚜 Refactor

- [**breaking**] Remove sender (name) from messages in event stream
- [**breaking**] Messages in Events now return sender_id


### 📚 Documentation

- Compare links in changelog


### ⚡ Performance

- Read and write tables concurrently if using file backed
- Start chat and session runtime simultaniously
- Reduce the number of full scans on all sessions
- Shutdown sessions store and chat simultaniously


## [0.1.1](https://github.com/pacman82/klatsch/compare/v0.1.0...v0.1.1) - 2026-04-23

### 🚀 Features

- Release binaries to GitHub

### 📚 Documentation

- Document different log targets
- Mention released artifacts in Readme

## 0.1.0 - 2026-04-22

### 🚀 Features

Initial release of Klatsch. No binary artifacts yet. Users would need to build from source. The server broadcasts messages between connected clients. Persistence is optional.

- *(ops)* Prevent two instances running using the same persistence directory
- *(ops)* Create persistence directory if it is missing.
- *(api)* Remember conversations by default between restarts. Transient mode is opt-in
- *(ui)* Login page
- *(ui)* Reconnect after sever shutdown
- *(ops)* Allow controlling log level with `LOG_LEVEL`. Allow controlling different levels 
- *(ui)* Distinguish between server and connection errors
- *(dev)* Introduce sabotage endpoint to cause events to respond with an SSE
- *(ui)* Communicate connection loss of events to users
- *(api)* Message conflicts are now forwarded by http api
- *(api)* Allow retry sending messages without fear of duplicates.
- *(ops)* Log used port
- *(ops)* Provide Dockerfile
- *(api)* Handle slow receivers gracefully
- *(ops)* Fast graceful shutdown
- *(api)* Messages are broadcasted to clients immediatly
- *(ui)* Add form for sending messages
- *(ops)* UI provided by the same process and port than api.
- *(ui)* Use coffee cup as favicon


### 📚 Documentation

- Readme
- Changelog
