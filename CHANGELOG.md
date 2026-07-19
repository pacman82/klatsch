# Changelog

`Klatsch` adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
## [0.2.0](https://github.com/pacman82/klatsch/compare/0.1.1...0.2.0) - 2026-07-19

### 🚀 Features

- 3 day sliding sessions expiration and 30 day max session duration
- Session max lifetime and idle timeout are configurable
- Sessions expire after 90 days at the latest
- Authentication required for users/{id} route
- Events route is authenticated
- [**breaking**] Sessions are invalidated after 30 days of inactivity
- Session are now truly revokable
- Remove session cookie on logout
- Remove sender from add_message body. Author is now identified via
- Add dummy session cookie after signup and login
- Separate user and sign up flow
- Add login http endpoint
- Show better error messages if logins fail
- *(ui)* Login button now always called Join, even if previous attempt
- Use password for authentication
- *(ui)* Log out automatically if current user is unknown
- *(ui)* Display 'Fetching user info ...' in UserBar if info not yet
- Registering user returns UUID
- Introduce route POST /api/v0/users
- Fetching User information with unknown id now yields 404.
- Route GET /users/<id>


### 🚜 Refactor

- [**breaking**] Remove sender (name) from messages in event stream
- [**breaking**] Messages in Events now return sender_id


### 📚 Documentation

- Compare links in changelog


### ⚡ Performance

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
