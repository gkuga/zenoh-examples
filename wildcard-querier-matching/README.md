# wildcard-querier-matching

A repro of wildcard queriers not reporting a match in `matching_status()` since zenoh 1.9.0.

Peer A declares queryables on `r/fam/one/status` and `r/fam/two/status`, and peer B declares queriers on one concrete key and three wildcards. For each, it prints `matching_status()`, whether `matching_listener` fired within 1 second, and the number of `get` replies.
A and B are two sessions in one process, connected over TCP on 127.0.0.1:17551, with multicast scouting disabled.

See [docs/03](../docs/03-wildcard-querier-matching.md) for the mechanism and [wildcard-query-retry](../wildcard-query-retry) for the fix.

## Run

```sh
make run         # zenoh 1.10.2 (Cargo.toml)
make run-1.8.0   # the same src/main.rs with zenoh 1.8.0 (zenoh-1.8.0/Cargo.toml)
make compare     # both
```

Cargo cannot put two semver-compatible versions of zenoh into one project, so the version is switched not by a feature but by a second manifest, `zenoh-1.8.0/Cargo.toml` (`[[bin]] path = "../src/main.rs"`).

## Output (2026-11-15, macOS)

zenoh 1.10.2:

```
zenoh v1.10.2-release
querier r/fam/one/status   matching_status=true  listener_event=Some(true)  get replies=1
querier r/fam/*/status     matching_status=false listener_event=None        get replies=2
querier r/**               matching_status=false listener_event=None        get replies=2
querier r/fam/one/*        matching_status=false listener_event=None        get replies=1
REPRODUCED: 3 querier(s) get replies but report no match: ["r/fam/*/status", "r/**", "r/fam/one/*"] (v1.10.2-release)
```

zenoh 1.8.0:

```
zenoh v1.8.0-release
querier r/fam/one/status   matching_status=true  listener_event=Some(true)  get replies=1
querier r/fam/*/status     matching_status=true  listener_event=Some(true)  get replies=2
querier r/**               matching_status=true  listener_event=Some(true)  get replies=2
querier r/fam/one/*        matching_status=true  listener_event=Some(true)  get replies=1
OK: every querier that gets replies reports a match (v1.8.0-release)
```

In 1.10.2 the three wildcards report `matching_status=false` and the listener never fires, yet `get` returns the same number of replies as on 1.8.0.
