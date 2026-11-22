N ?= 200000
PORT ?= 17501
DEADLINE ?= 30
LINGER ?= 10

# Session A, on this host. ENDPOINT is the address B connects to.
ENDPOINT ?= tcp/0.0.0.0:$(PORT)
serve:
	cargo run --release -- serve $(N) $(ENDPOINT)

# Session B, on this host. Set PJS_REMOTE to A's endpoint, e.g. tcp/<A's address>:17501.
join:
	cargo run --release -- $(N) $(DEADLINE) $(LINGER)

# Builds one binary against a zenoh git tree at REV, in a copy of this project
# under BUILD_DIR/VARIANT (Cargo.toml is edited there, not here):
#   make build-variant ZENOH_GIT=<git url> REV=<commit> VARIANT=<name>
# TARGET is a Rust target triple; the default builds for arm64 Linux with glibc 2.31
# (cargo-zigbuild). Use TARGET= to build for this host with plain cargo.
BUILD_DIR ?= $(HOME)/pjr-build
CARGO_TARGET_DIR ?= $(HOME)/pjr-target
TARGET ?= aarch64-unknown-linux-gnu.2.31
build-variant:
	@test -n "$(ZENOH_GIT)" && test -n "$(REV)" && test -n "$(VARIANT)" || \
		{ echo "set ZENOH_GIT, REV and VARIANT"; exit 1; }
	mkdir -p $(BUILD_DIR)/$(VARIANT)
	cp -r Cargo.toml Cargo.lock src $(BUILD_DIR)/$(VARIANT)/
# -i.bak works with both GNU and BSD sed.
	sed -i.bak 's|^zenoh = { version = "[^"]*",|zenoh = { git = "$(ZENOH_GIT)", rev = "$(REV)",|' \
		$(BUILD_DIR)/$(VARIANT)/Cargo.toml && rm $(BUILD_DIR)/$(VARIANT)/Cargo.toml.bak
	grep -q 'rev = "$(REV)"' $(BUILD_DIR)/$(VARIANT)/Cargo.toml
	cd $(BUILD_DIR)/$(VARIANT) && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_NET_GIT_FETCH_WITH_CLI=true \
		$(if $(TARGET),cargo zigbuild --release --target $(TARGET),cargo build --release)
	cp $(CARGO_TARGET_DIR)/$(if $(TARGET),$(firstword $(subst ., ,$(TARGET)))/,)release/peer-join-remote \
		$(BUILD_DIR)/$(VARIANT)/peer-join-remote
	@echo "built $(BUILD_DIR)/$(VARIANT)/peer-join-remote"

clean:
	cargo clean

.PHONY: serve join build-variant clean
