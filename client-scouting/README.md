# client-scouting

Two defaults that keep a client-mode session from connecting to a peer, and how to fix them (zenoh 1.8.0). A single run tries both the failing settings and the fixed ones.

**Confirmed on:** zenoh 1.8.0 (2026-11-15), both parts; zenoh 1.10.2 (2026-11-17), only part 2. Since 1.9.0 the client's default `autoconnect` includes peers, so part 1 no longer fails; see [On 1.9.0 and later](#on-190-and-later). macOS.

1. The client looks, via scouting, for a peer that answers multicast scouting
   - a. `scouting/multicast/autoconnect` at its default (a client only connects to routers)
   - b. `autoconnect: { client: ["router", "peer"] }`
2. The client connects via `connect/endpoints` (no scouting) to a peer that starts listening on 127.0.0.1:17572 after 1 second
   - a. `connect/timeout_ms` at its default (0 for a client = try only once)
   - b. `connect/timeout_ms: -1` (keep trying until connected)

Once connected, the client does a `get` on `demo/hello` and checks that the peer's queryable replies.

Multicast uses `224.0.0.224:17571` instead of the default `224.0.0.224:7446` (configurable by arguments: `cargo run --release -- 224.0.0.224:17581 17582`). The peer only listens on 127.0.0.1. See [docs/04](../docs/04-client-scouting.md) for details.

## Run

```sh
make run
```

## Output (2026-11-15, macOS)

```
peer answering scouting on 224.0.0.224:17571
1a. scouting, default autoconnect: open failed after 3.00s: timeout at $CARGO_HOME/registry/src/index.crates.io-…/zenoh-1.8.0/src/net/runtime/orchestrator.rs:1093.
1b. scouting, autoconnect {client:["router","peer"]}: open after 3.31ms, 1 reply from demo/hello after 3.83ms
2a. connect, default timeout_ms (0): open failed after 388.54µs: Unable to connect to any of [tcp/127.0.0.1:17572]!  at $CARGO_HOME/registry/src/index.crates.io-…/zenoh-1.8.0/src/net/runtime/orchestrator.rs:374.
late peer listening on tcp/127.0.0.1:17572
2b. connect, timeout_ms -1: open after 1.01s, 1 reply from demo/hello after 1.01s
OK: the defaults fail (no peer autoconnect; no wait for the endpoint) and both fixes connect (scouted peer in 3.31ms, late endpoint in 1.01s)
```

- 1a finds the peer but does not connect, and `zenoh::open` fails after `scouting/timeout` (3 seconds). 1b connects in a few ms.
- 2a fails immediately because the other side is not there yet. 2b's open returns on the retry after the other side starts listening. The retry interval follows the `connect/retry` default (start at 1 s, double each time, up to 4 s), so if the 1 s retry is too early for the other side's listen, it takes 3 s (another run on the same day gave `late endpoint in 3.01s`).

## On 1.9.0 and later

zenoh 1.9.0 changed the client's default `scouting/multicast/autoconnect` from `"router"` to `"router|peer|client"` (`zenoh-config/src/defaults.rs`). A client now connects to a peer it finds by scouting, so 1a succeeds. The default `connect/timeout_ms` of a client is still `0`, so 2a still fails. Built against 1.10.2 (2026-11-17, macOS), the sample prints:

```
peer answering scouting on 224.0.0.224:17571
1a. scouting, default autoconnect: open after 2.06ms, 1 reply from demo/hello after 2.40ms
1b. scouting, autoconnect {client:["router","peer"]}: open after 900.33µs, 1 reply from demo/hello after 1.05ms
2a. connect, default timeout_ms (0): open failed after 247.29µs: Unable to connect to any of [Single(tcp/127.0.0.1:17572)]!  at $CARGO_HOME/registry/src/index.crates.io-…/zenoh-1.10.2/src/net/runtime/orchestrator.rs:447.
late peer listening on tcp/127.0.0.1:17572
2b. connect, timeout_ms -1: open after 1.00s, 1 reply from demo/hello after 1.00s
UNEXPECTED: default autoconnect ok=true, peer autoconnect ok=true, timeout 0 ok=false, timeout -1 ok=true
```

`UNEXPECTED` means only that 1a did not fail as it does on 1.8.0. Setting `autoconnect` explicitly, as in 1b, keeps the behavior the same on both versions.
