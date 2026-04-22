# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
## [0.1.0] - 2026-04-22

### 🚀 Features

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

# Changelog
