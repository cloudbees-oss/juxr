FROM ekidd/rust-musl-builder:1.48.0 AS build

ADD --chown=rust:rust . ./

ARG VERSION

RUN set -xe ; \
    test -z "$VERSION" || sed -i -e "/\[package]/,/\[dependencies]/{s/version = \".*\"/version= \"$VERSION\"/}" Cargo.toml ; \
    cargo install --target x86_64-unknown-linux-musl --path .

# Now for the runtime image
FROM alpine

ARG BUILD_DATE
ARG VCS_REF
ARG VERSION
LABEL org.label-schema.build-date=$BUILD_DATE \
          org.label-schema.name="JUnit XML Reporting Toolkit" \
          org.label-schema.description="A command line tool for helping manage JUnit XML formatted reports." \
          org.label-schema.url="https://cloudbees-oss.github.io/juxr/" \
          org.label-schema.vcs-ref=$VCS_REF \
          org.label-schema.vcs-url="https://github.com/cloudbees-oss/juxr" \
          org.label-schema.version=$VERSION \
          org.label-schema.schema-version="1.0"

COPY --from=build /home/rust/.cargo/bin/juxr /usr/local/bin/juxr

ENTRYPOINT ["/user/local/bin/juxr"]
CMD ["help"]
