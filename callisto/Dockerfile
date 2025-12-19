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

# Replace with real src
RUN rm -rf ./src
COPY . .

# do a release build
FROM build AS release
ARG TLS_UPGRADE=1

RUN touch ./src/main.rs
RUN touch ./src/lib.rs
RUN if [ "$TLS_UPGRADE" = "1" ]; then cargo build --release; else cargo build --release --features no_tls_upgrade; fi
#RUN strip target/release/callisto

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
COPY --from=build /app/scenarios* /scenarios
COPY --from=build /app/ship_templates* /ship_templates
COPY --from=build /app/config* /config

ENV RUST_LOG=info,gomez=warn

# set the binary as entrypoint
ENTRYPOINT ["/callisto", "-p", "30000"]
# Used for debugging the image
# ENTRYPOINT ["tail", "-f", "dev/null"]