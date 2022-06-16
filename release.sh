# Craft builds with linux-gnu by default
# (openssl-sys crate does not handle musl well right now)
cargo build --release --target=x86_64-unknown-linux-gnu --locked
