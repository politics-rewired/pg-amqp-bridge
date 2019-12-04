FROM ekidd/rust-musl-builder as builder

WORKDIR /home/rust/

# Avoid having to install/build all dependencies by copying
# the Cargo files and making a dummy src/main.rs
COPY Cargo.toml .
COPY Cargo.lock .
RUN echo "fn main() {}" > src/main.rs  \
  && mkdir tests && echo "fn main() {}" > tests/main.rs  \
  && cargo build --release

COPY . .

# We need to touch our real main.rs file or else docker will use the cached one.
RUN sudo touch src/main.rs  \
  && cargo build --release

# Size optimization
RUN strip target/x86_64-unknown-linux-musl/release/pg-amqp-bridge

# Start building the final image
FROM scratch
WORKDIR /home/rust/
COPY --from=builder /home/rust/target/x86_64-unknown-linux-musl/release/pg-amqp-bridge .
ENTRYPOINT ["./pg-amqp-bridge"]
