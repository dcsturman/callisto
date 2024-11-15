# Start with a rust alpine image
FROM rust:1-alpine3.20
# if needed, add additional dependencies here
RUN apk add --no-cache musl-dev
# set the workdir and copy the source into it
WORKDIR /app
COPY ./ /app

# do a release build
RUN cargo build --release
RUN strip target/release/callisto

# use a plain alpine image, the alpine version needs to match the builder
FROM alpine:3.20
# Expose our port
EXPOSE 8080

# if needed, install additional dependencies here
RUN apk add --no-cache libgcc
# copy the binary into the final image
COPY --from=0 /app/target/release/callisto .
COPY ./scenarios /scenarios
# set the binary as entrypoint
ENTRYPOINT ["/callisto", "-s", "/scenarios/scenario2.json", "-p", "8080"]
