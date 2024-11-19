# Start with a rust alpine image
FROM rust:latest AS base
# if needed, add additional dependencies here
RUN rustup update

# set the workdir
FROM base AS build

WORKDIR /app

# Build dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir ./src && echo 'fn main() {}' > ./src/main.rs && touch ./src/lib.rs
RUN cargo build --release

# Replace with real src
RUN rm -rf ./src
COPY . .

# do a release build
FROM build AS release
RUN touch ./src/main.rs
RUN touch ./src/lib.rs
RUN cargo build --release
#RUN strip target/release/callisto

# use a plain alpine image, the alpine version needs to match the builder
FROM debian:bookworm-slim AS run
RUN apt-get update && apt-get install -y ca-certificates

# Expose our port
EXPOSE 30000

# if needed, install additional dependencies here
# RUN apk add --no-cache libgcc
# copy the binary into the final image
COPY --from=release /app/target/release/callisto /callisto
COPY --from=build /app/scenarios /scenarios
COPY --from=build /app/config /config

ENV RUST_LOG=warn
# set the binary as entrypoint
ENTRYPOINT ["/callisto", "-s", "/scenarios/scenario2.json", "-p", "30000"]