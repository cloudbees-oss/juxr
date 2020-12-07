FROM ekidd/rust-musl-builder:1.48.0 AS build

ADD --chown=rust:rust . ./

ARG VERSION

RUN set -xe ; \
    test -z "$VERSION" || sed -i -e "/\[package]/,/\[dependencies]/{s/version = \".*\"/version= \"$VERSION\"/}" Cargo.toml ; \
    cargo install --target x86_64-unknown-linux-musl --path .

# Now for the runtime image
FROM alpine

COPY --from=build /home/rust/.cargo/bin/juxr /usr/local/bin/juxr

ENTRYPOINT ["/user/local/bin/juxr"]
CMD ["help"]
