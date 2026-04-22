# Changelog

`Klatsch` adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
## [0.1.1] - 2026-04-22

### 🚀 Features

- Please release-plz, create a release PR
- Log causes for errors as "error" attribute in log messages
- Prevent two instances running using the same persistence directory

  using lockfile

- Create database directory if it is missing.
- [**breaking**] Make persistence the default. Transient mode is opt-in
- [**breaking**] Configure persistence directory instead of database file path
- Use persistence target in logging
- Map axum::serve target to server in log output
- Login page
- *(ui)* Reconnect after sever shutdown
- Allow controlling log level with `LOG_LEVEL`. Separate targets for
- Statically linked runtime for windows builds
- Persistence
- *(ui)* Distinguish between server and connection errors
- Sabotage events mid stream.
- Introduce sabotage endpoint to cause events to respond with an SSE
- *(ui)* Communicate connection loss of events to users
- Communicate send errors to the user in UI
- Conflicts are now forwarded by http api
- *(ui)* Allow retry sending messages without fear of duplicates.
- Api rejects duplicate messages.
- Log used port
- Add Dockerfile
- ConversationsClient now supports slow receivers
- Fast graceful shutdown
- Move user picker to the upper right corner
- Active user can now be controlled in UI. (AI vibe coded)
- Messages are now broadcasted to UI immediatly
- Supress log output from memory_serve
- Rename to klatsch
- Send messages as Bob
- *(ui)* Add form for sending messages to UI
- Change max log level to info
- *(ui)* Tattle is now page title
- Add uuid to messages returned from messages route
- Return hardcoded messages from messages route
- Introduce empty route `messages`.
- Stub message container in Frontend
- Statically host ui in server
- Graceful shutdown
- Minimal server displaying "Hello, World!"
- Use coffee cup as favicon


### 🐛 Bug Fixes

- Log to standard error instead of standard out
- Klatsch no longer panics if the last_event_id send by UI exceeds


### 📚 Documentation

- Mention released artifacts in Readme
- Document different log targets
- Update Readme
- Update Readme
- .env.example now correctly states that 3000 is the default port,
- Remove Milestones from Readme
- Update readme


### ⚡ Performance

- Use more chipset features
- Use entry api instead of recalculating the hash then inserting
- Use lookup into HashSet to check for duplicate messages.
- Register routes asynchronously

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
