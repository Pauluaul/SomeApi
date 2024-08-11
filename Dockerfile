FROM messense/rust-musl-cross:x86_64-musl as builder
ENV SQLX_OFFLINE=true
WORKDIR /some_api
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM scratch
COPY --from=builder /some_api/target/x86_64-unknown-linux-musl/release/some_api /some_api
COPY src/frontend /src/frontend
ENTRYPOINT ["/some_api"]
EXPOSE 3000