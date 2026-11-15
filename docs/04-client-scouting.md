# Client, scouting and connect

Sample: [client-scouting](../client-scouting) (repro and fix in one)

A client-mode session has several defaults that differ from a peer's. These are two pitfalls when switching from peer to client ([01](01-peer-join-declarations.md)).

## 1. It does not connect to a peer found by scouting

Defaults of `scouting/multicast/autoconnect` (`zenoh-config/src/defaults.rs`):

| Mode | Default | Meaning |
| --- | --- | --- |
| router | `""` | Does not connect automatically |
| peer | `"router\|peer"` | Connects to routers and peers |
| client | `"router"` | **Connects to routers only** |

Even when a client finds a peer by multicast scouting, it does not connect, and `zenoh::open` fails after `scouting/timeout` (3 seconds). To also connect to peers:

```json5
scouting: { multicast: { autoconnect: { client: ["router", "peer"] } } }
```

Measured (2026-11-15): with the default it fails after 3.00 s; with the setting above it opens in 3.3 ms and gets a reply (12 ms in another run).

## 2. It fails immediately against an endpoint that is not listening yet

`connect/timeout_ms` defaults to `-1` for routers and peers (keep trying until connected) and to **`0`** for clients (try once; if it fails, `zenoh::open` returns an error). `connect/exit_on_failure` is also `true` only for clients.
If the client starts before the other side, it fails immediately with `Unable to connect to any of [tcp/127.0.0.1:...]!`.

With

```json5
connect: { endpoints: ["tcp/127.0.0.1:17572"], timeout_ms: -1 }
```

open waits until the other side is listening.

Measured (2026-11-15): against an endpoint that starts listening after 1 s, the default fails in 0.4 ms and `-1` opens in 1.01 s (retries follow the `connect/retry` default: start at 1 s, double each time, up to 4 s, so if the 1 s retry is too early it takes 3 s).

## Keeping multicast local

client-scouting uses `224.0.0.224:17571` instead of the default group `224.0.0.224:7446`, and the peer only listens on 127.0.0.1. The TTL is the default 1, so packets do not cross routers.
On macOS, setting `scouting/multicast/interface` to `"lo0"` broke scouting (the client could not find the peer), so the interface stays at the default `"auto"`. The scouting packets themselves do go out on the LAN, but only this sample listens on that port.
