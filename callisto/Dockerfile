# Start with a rust alpine image
FROM rust:alpine AS base
# Install build dependencies for Alpine
RUN apk add --no-cache musl-dev pkgconfig openssl-dev openssl-libs-static
RUN rustup update

# set the workdir
FROM base AS build

WORKDIR /app

# Build dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir ./src && echo 'fn main() {}' > ./src/main.rs && touch ./src/lib.rs
RUN cargo build --release --features no_tls_upgrade

# Replace the stub with the real code. Only the code: the scenarios, ship
# templates and config travel in a stage of their own below, so editing one of
# them does not invalidate this layer and force the crate to compile again.
RUN rm -rf ./src
COPY src ./src

# do a release build
FROM build AS release
# Default to TLS enabled so ad-hoc/manual image builds don't silently disable it.
# Local docker-compose development can explicitly override this build arg.
ARG TLS_UPGRADE=1

RUN touch ./src/main.rs
RUN touch ./src/lib.rs
RUN if [ "$TLS_UPGRADE" = "1" ]; then cargo build --release; else cargo build --release --features no_tls_upgrade; fi
#RUN strip target/release/callisto

# The data the image serves, gathered after the compile so that editing a
# scenario costs a few seconds rather than a full rebuild.
FROM base AS data
WORKDIR /app
COPY . .

# use a plain alpine image, the alpine version needs to match the builder
FROM alpine:latest AS run
RUN apk add --no-cache ca-certificates libgcc openssl

# Expose our port
EXPOSE 30000

# if needed, install additional dependencies here
# RUN apk add --no-cache libgcc
# copy the binary into the final image
COPY --from=release /app/target/release/callisto /callisto
# A little bit of a hack to copy these directories only if they exists (config won't exist with github actions)
COPY --from=data /app/scenarios* /scenarios
COPY --from=data /app/ship_templates* /ship_templates
COPY --from=data /app/config* /config

ENV RUST_LOG=info,gomez=warn

# set the binary as entrypoint
ENTRYPOINT ["/callisto", "-p", "30000"]
# Used for debugging the image
# ENTRYPOINT ["tail", "-f", "dev/null"]
