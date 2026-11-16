# ExprId exhaustion

Samples: [expr-id-exhaustion](../expr-id-exhaustion) (repro), [expr-id-exhaustion-fork](../expr-id-exhaustion-fork) (fix in a fork)

## What an ExprId is

Instead of sending a key expression as a string every time, Zenoh assigns it a number per link (face), tells the other side with `DeclareKeyExpr`, and from then on sends the number.
That number is the `ExprId`: `pub type ExprId = u16;` in `zenoh-protocol/src/core/wire_expr.rs`. 0 is reserved for "no prefix", so one link can use 1 to 65 535.
A number is only freed when the other side undeclares it or when the face closes ([#2825](https://github.com/eclipse-zenoh/zenoh/issues/2825)).

## Allocation in 1.8.0

`zenoh/src/net/routing/dispatcher/face.rs`:

```rust
pub(crate) fn get_next_local_id(&self) -> ExprId {
    let mut id = 1;
    while self.local_mappings.contains_key(&id) || self.remote_mappings.contains_key(&id) {
        id += 1;
    }
    id
}
```

- It searches from 1 every time, so with k ids in use it checks k times, and sending N declarations is O(N²). This runs inside the routing table's write lock (it is called from `Resource::decl_key`).
- When all 65 535 are in use:
  - A debug build panics on the overflow of `id += 1`.
  - A release build wraps around to 0. 0 is not in the maps, so the 65 536th declaration is given the reserved 0.
  - For the 65 537th, 0 is in use too, so the loop never ends. Since it holds the write lock, every declaration and `put` of that session stalls.

expr-id-exhaustion checks that it stalls exactly at the **65 537th declaration**.

## Why liveliness tokens instead of queryables

In 1.8.0, `declare_queryable` and `declare_subscriber` first declare the non-wildcard part of the key with the **session's own** u16 counter (`expr_id_counter` in `Session`, `// @TODO: manage rollover and uniqueness`) (`declare_nonwild_prefix` → `declare_prefix`).
That counter is a plain `fetch_add`, so it wraps at the 65 536th declaration, the router logs `Resource 1 remapped. Remapping unsupported!`, and later declarations are mapped onto existing resources. They no longer need new numbers on the face, so the face never gets exhausted.

Declaring 70 000 queryables from one session (2026-11-15, release):

- All declarations returned in 108 s (no hang)
- `Remapping unsupported!` was logged 4 464 times (= 70 000 − 65 536)
- A `get` from B for the last key `demo/decl/69999` got no reply

So declaring 65 536 or more queryables from one session does not hang, but silently breaks.
Face exhaustion happens when declarations from several sessions (for example several clients connected to a router or a peer) are funneled into one link. Liveliness tokens do not declare a prefix and are sent with the full key (`declare_liveliness_inner`), so they reproduce it from a single session.

## The fork's fix

The fork's tag `1.8.0+gkuga.1` ([`a574569`](https://github.com/gkuga/zenoh/commit/a574569c51a25ecec6ebe5d4059aff156296ec80), branch `gkuga/1.8`) is 1.8.0 plus two code commits, described in the fork's `PATCHES.md`:

- **A wrapping cursor.** Numbers come from a cursor that skips 0, and numbers freed by the other side's undeclare are reused first. Candidates are still checked against both maps. Amortized O(1).
- **Handling exhaustion.** When every number is in use, `decl_key` sends the full key expression without assigning a number. The face remembers that it is exhausted and does not search again. The warning is logged once per face (`No expression id left, sending ... and later key expressions in full`). An undeclare from the other side clears the flag.

The wire format does not change (`ExprId` stays u16). To use it, add this to Cargo.toml:

```toml
[patch.crates-io]
zenoh = { git = "https://github.com/gkuga/zenoh", tag = "1.8.0+gkuga.1" }
```

zenoh depends on the other `zenoh-*` crates in its repository by path, so every `zenoh-*` in `Cargo.lock` comes from this commit.

This fix keeps a node from hanging on exhaustion; the push under the lock from [01](01-peer-join-declarations.md) remains. Measured on the fork side (control queue 16), the time for a peer joining a session with N declarations to get its first reply went from 4.55 s to 3.17 s at 10k and from 33.1 s to 26.2 s at 20k.
