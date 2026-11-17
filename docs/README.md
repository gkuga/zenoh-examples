# Zenoh investigation notes

How the problems reproduced by the samples in this repository work.
Source locations refer to each version as fetched from crates.io (for example `~/.cargo/registry/src/*/zenoh-1.8.0/`), which corresponds to <https://github.com/eclipse-zenoh/zenoh/tree/1.8.0> on GitHub.

## Contents

1. [Peer join and the full push of declarations](01-peer-join-declarations.md) — peer-join-stall / client-join. In p2p, all declarations are sent to a joining peer while holding the write lock. A client only receives declarations matching its interests
2. [ExprId exhaustion](02-expr-id-exhaustion.md) — expr-id-exhaustion. Linear search in `get_next_local_id`, the u16 limit, the wraparound of the session-side counter, and a fix
3. [Wildcard querier matching](03-wildcard-querier-matching.md) — wildcard-querier-matching / wildcard-query-retry. Since 1.9.0, `remote_queryable_matching_status` requires inclusion
4. [Client, scouting and connect](04-client-scouting.md) — client-scouting. The client defaults of `scouting/multicast/autoconnect` and `connect/timeout_ms`
5. [Cost of a put to an undeclared key](05-put-route-cost.md) — put-route-cost / put-route-fix. A key without a resource has no cached route, so `Resource::get_matches` runs on every put

## Related upstream issues / PRs (as of 2026-11-14)

| Number | Summary | State |
| --- | --- | --- |
| [eclipse-zenoh/zenoh#2825](https://github.com/eclipse-zenoh/zenoh/issues/2825) | The router hangs when a face runs out of u16 ExprIds | open |
| [eclipse-zenoh/zenoh#2835](https://github.com/eclipse-zenoh/zenoh/pull/2835) | Reclaim ExprIds and stop spinning on exhaustion | open |
| [eclipse-zenoh/zenoh#2709](https://github.com/eclipse-zenoh/zenoh/pull/2709) | Fail pushes to a transport being closed as unresponsive immediately | merged, included in 1.10.2 |
| [logivations/zenoh#8](https://github.com/logivations/zenoh/pull/8) | Another fork. Reports the same hang and makes sending in `decl_key` non-blocking | — |
