# Peer join and the full push of declarations

Samples: [peer-join-stall](../peer-join-stall) (repro), [client-join](../client-join) (fix)

## What happens

When another session B joins, as a **peer** over TCP, a session A that holds many declarations such as queryables:

- B's first `get` gets no reply (with a large N, no reply arrives no matter how long it waits)
- A's own `put` stalls
- After 5 seconds A closes the link (`Unable to push non droppable network message to ... Closing transport!`), B reconnects, and the same thing repeats

When B joins the same A as a **client**, it gets a reply in a few ms even with 40k queryables.

## Mechanism (zenoh 1.8.0)

1. **In p2p, all declarations are sent to a joining peer.** `zenoh/src/net/routing/hat/p2p_peer/mod.rs` says "at connection … peers act as if they received an interest CurrentFuture with id 0", and `queries_new_face` and friends send every existing queryable to the new face.
2. **This is done while holding the routing table's write lock.** For each declaration, `Resource::decl_key` emits a `DeclareKeyExpr`, and declarations use `CongestionControl::Block`, so sending blocks until the control queue (`transport/link/tx/queue/size/control`, 2 batches by default) has room.
3. **The new link's read loop does not start until the join has finished.** `zenoh-transport/src/unicast/manager.rs` says "the read loop … remains blocked until new_unicast() returns". Once the queue is full, the join keeps waiting and never finishes.
4. **Local `put`s wait on the same lock.** `route_data` takes the table's read lock, so A's `put` does not return while the join is in progress.
5. **If pushing to the queue stays blocked for longer than `wait_before_close` (5 seconds), the transport is closed** (`handle_push_result` in `zenoh-transport/src/unicast/universal/tx.rs`). B reconnects according to `connect/endpoints`, and everything starts over.
6. On top of that, ExprId allocation in `FaceState::get_next_local_id` searches linearly from 1, so sending N declarations is O(N²) ([02](02-expr-id-exhaustion.md)).

Increasing `transport/link/tx/queue/size/control` (max 16) or `wait_before_close` shifts the N at which things start to stall, but does not change the shape of the curve.

## Why a client is fast

Since zenoh 1.0, declarations are no longer propagated to client and peer subsystems, and a client only receives declarations that match the interests it sent ([zenoh 1.0 release post](https://zenoh.io/blog/2024-10-21-zenoh-firesong/): "subscribers, queryables and liveliness tokens declarations are not propagated any more to clients and peers subsystems").
B in client-join has no declarations or interests and only does a `get`, so A sends no declarations when it joins.

To use a client:

- A listens on a fixed endpoint (`listen/endpoints: ["tcp/127.0.0.1:17511"]`)
- B uses `mode: "client"` and lists A in `connect/endpoints`
- `scouting/multicast/enabled: false` (by default a client does not connect to peers found by scouting; see [04](04-client-scouting.md))

## Measurements (2026-11-15, macOS, release)

Time from B joining until its first reply:

| N (queryables) | peer | client |
| --- | --- | --- |
| 1 000 | 0.50 s | — |
| 10 000 | 1.01 s | 3 ms |
| 20 000 | 5.40 s | 3 ms |
| 30 000 | no reply within 30 s, link closed 6 times | — |
| 40 000 | no reply within 30 s, A's `put` stalled for 30 s, link closed 6 times | 5–8 ms |

The 0.5 s for a peer is the time a peer's `zenoh::open` takes to return after waiting for scouting (`scouting/delay`, 500 ms).
In another environment (an arm64 edge device), a peer took about 11 s at 10 000 and got no reply within 15 s at 20 000. The N at which it stalls depends on the machine.

The time for A to declare the N queryables was itself O(N²) as well (1.1 s at 10 000, 5.2 s at 20 000, 21 s at 40 000), because `Session::declare_prefix` in 1.8.0 searches `local_resources` linearly (`zenoh/src/api/session.rs`).
