N ?= 70000

# Release build: a debug build panics on the u16 overflow instead of hanging.
run:
	cargo run --release -- $(N)

clean:
	cargo clean
