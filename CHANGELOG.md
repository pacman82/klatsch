# Changelog

`Klatsch` adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-04-23

### 🚀 Features

- Release binaries to GitHub

### 📚 Documentation

- Document different log targets
- Mention released artifacts in Readme

## [0.1.0] - 2026-04-22

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
