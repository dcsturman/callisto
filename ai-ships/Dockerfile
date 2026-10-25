# The AI service. Build from the repository root, because the service links
# the backend crate as a library:
#
#   docker build -f ai-ships/Dockerfile .
#
# ai-ships/Dockerfile.dockerignore keeps the rest of the repository (the
# frontend, build output) out of the context.
FROM rust:alpine AS base
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
RUN rustup update

FROM base AS build
WORKDIR /app

# Dependencies first, against stub sources, so a code change does not rebuild
# every crate.
COPY callisto/Cargo.toml callisto/Cargo.lock callisto/
COPY ai-ships/Cargo.toml ai-ships/Cargo.lock ai-ships/
RUN mkdir -p callisto/src ai-ships/src \
 && echo 'fn main() {}' > callisto/src/main.rs && touch callisto/src/lib.rs \
 && echo 'fn main() {}' > ai-ships/src/main.rs && touch ai-ships/src/lib.rs
WORKDIR /app/ai-ships
RUN cargo build --release

# Then the real code.
WORKDIR /app
RUN rm -rf callisto/src ai-ships/src
COPY callisto/src callisto/src
COPY ai-ships/src ai-ships/src
# The trained network the service plays with, built into the binary.
COPY ai-ships/networks ai-ships/networks
WORKDIR /app/ai-ships
RUN touch src/main.rs src/lib.rs ../callisto/src/lib.rs && cargo build --release

FROM alpine:latest AS run
RUN apk add --no-cache ca-certificates libgcc
COPY --from=build /app/ai-ships/target/release/ai-ships /ai-ships
ENV RUST_LOG=info
EXPOSE 8080
ENTRYPOINT ["/ai-ships"]
