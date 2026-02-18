# 1: Build the exe
FROM rust:latest AS chef
WORKDIR /usr/src/klatsch
# We link everything, including libc into one executable. This way we can construct `runner` by
# copying our executable into an empty scratch image. We want this to work for both arm and x86
# targets. We identify our default host target triplet to determine which architecture we run on.
RUN HOST_TUPLE=$(rustc --print host-tuple); \
    TARGET=$(echo "$HOST_TUPLE" | cut -d'-' -f1); \
    TARGET_TRIPLE="${TARGET}-unknown-linux-musl"; \
    # Save target triple to file, so we can use it in other stages
    echo "$TARGET_TRIPLE" > "/target_triple.txt"; \
    rustup target add "$TARGET_TRIPLE";
# Cargo Chef is used to cache the building of dependencies
RUN cargo install cargo-chef
# Cargo Auditable is used to enrich the executable with metainformation about our build
# dependencies. These can be used to scan for vulnerabilities later on
RUN cargo install cargo-auditable
RUN apt-get update && apt-get install -y \
    # Node.js and npm for building the SvelteKit UI
    nodejs npm \
    # musl-gcc for compiling the bundled SQLite against musl libc
    musl-tools \
    && rm -rf /var/lib/apt/lists/*

FROM chef AS planner
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=chef /target_triple.txt target_triple.txt
COPY --from=planner /usr/src/klatsch/recipe.json recipe.json
# Build dependencies
RUN cargo chef cook --release --recipe-path recipe.json --target $(cat target_triple.txt)
COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src
COPY ui ./ui
# Build the application (build.rs handles the UI build via npm)
RUN cargo auditable build --release --target $(cat target_triple.txt)

# 2: Copy the executable to an empty Docker image
FROM scratch AS runner
COPY --from=builder /usr/src/klatsch/target/*/release/klatsch .
# Run as unprivileged user. If klatsch would be subject to an exploitable vulnerability this limits
# the damage an attacker could do.
USER 1000
# Klatsch exposes its UI over port 3000 by default
EXPOSE 3000
ENTRYPOINT ["./klatsch"]
