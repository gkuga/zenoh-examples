# zenoh 1.10.2 (Cargo.toml)
run:
	cargo run --release

# The same src/main.rs against zenoh 1.8.0 (zenoh-1.8.0/Cargo.toml)
run-1.8.0:
	cargo run --release --manifest-path zenoh-1.8.0/Cargo.toml

compare: run-1.8.0 run

clean:
	cargo clean
	cargo clean --manifest-path zenoh-1.8.0/Cargo.toml
