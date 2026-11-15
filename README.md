# zenoh-examples

Minimal samples that reproduce problems I ran into with [Zenoh](https://zenoh.io/), plus samples that show how to fix them. Written in Rust.
Each sample is its own Cargo project (each pins its own zenoh version, so an empty `[workspace]` keeps it out of any parent workspace). Run `make run` inside a sample's directory.

The mechanisms behind each problem are explained in [docs](docs/README.md).

| Directory | Kind | zenoh | What it shows |
| --- | --- | --- | --- |
| [peer-join-stall](peer-join-stall) | Repro | 1.8.0 | When a **peer** joins a session that holds many declarations, the first reply never arrives, the other side's `put` stalls, and the link is closed every 5 seconds |
| [client-join](client-join) | Fix | 1.8.0 | Same setup, but the joining side is a **client**: replies come back in a few ms even with 40k queryables |
| [expr-id-exhaustion](expr-id-exhaustion) | Repro | 1.8.0 | Using up the u16 ExprIds (65 535) on one link makes declarations never return (release build) |
| [expr-id-exhaustion-fork](expr-id-exhaustion-fork) | Fix (fork) | 1.8.0 + patch | With a fork (gkuga/zenoh `nue-1.8.0-expr-id-2`) via `[patch.crates-io]`, exhausting the ExprIds no longer hangs |
| [wildcard-querier-matching](wildcard-querier-matching) | Repro | 1.10.2 / 1.8.0 | Since 1.9.0, `matching_status()` of a wildcard querier stays false (although `get` gets replies). `make compare` compares with 1.8.0 |
| [wildcard-query-retry](wildcard-query-retry) | Fix | 1.10.2 | Don't wait for matching: retry a `get` with a timeout, or query concrete keys |
| [client-scouting](client-scouting) | Repro + fix | 1.8.0 | A client does not connect to a **peer** it found by scouting (autoconnect), and fails immediately against an endpoint that is not listening yet (`connect/timeout_ms`) |
| [put-route-cost](put-route-cost) | Repro | 1.8.0 | A `put` to an undeclared key has no cached route and intersects with every declaration each time. With 20k declared keys a put goes from 0.35 µs to 814 µs |
| [put-route-fix](put-route-fix) | Fix | 1.8.0 | (a) put through a per-key `Publisher` (stays at 0.1 µs), (b) batch many values into one put |

## Common structure

- Every sample is a single `main.rs`. It prints what happens, exits on its own, and ends with a verdict line such as `REPRODUCED: ...` or `OK: ...`.
- Closing or dropping a Zenoh session can take about 10 seconds, so the samples exit with `std::process::exit(0)`.
- All traffic is TCP on 127.0.0.1. Each sample uses fixed ports (17501–17582), so they can run at the same time without clashing. Multicast scouting is disabled everywhere except client-scouting, which is about scouting.
- Some samples measure time, so `make run` is `cargo run --release`. The first build takes 1–2 minutes.
- Measurements were taken on 2026-11-15 on macOS (Apple M3 Pro, 11 cores, Rust 1.96.0).

## Background

An edge application declared a Zenoh queryable and subscribers for every measurement point, and the daemon hung once it reached a few thousand points. The problems found while investigating that are extracted here.
