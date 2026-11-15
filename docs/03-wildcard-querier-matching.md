# Wildcard querier matching

Samples: [wildcard-querier-matching](../wildcard-querier-matching) (repro, `make compare` compares with 1.8.0), [wildcard-query-retry](../wildcard-query-retry) (fix)

## What happens

Peer A declares queryables on `r/fam/one/status` and `r/fam/two/status`, and peer B declares queriers.

| Querier key | `matching_status()` in 1.8.0 | `matching_status()` in 1.10.2 | `get` replies (both) |
| --- | --- | --- | --- |
| `r/fam/one/status` | true | true | 1 |
| `r/fam/*/status` | true | **false** | 2 |
| `r/**` | true | **false** | 2 |
| `r/fam/one/*` | true | **false** | 1 |

In 1.10.2, a wildcard querier's `matching_status()` is false and its `matching_listener` never fires, yet `get` itself does get replies.
1.9.0, 1.10.0, 1.10.1 and 1.10.2 give the same result; only 1.8.0 reports true. The Makefile in this repository only compares 1.10.2 and 1.8.0; 1.9.0–1.10.1 were checked on 2026-11-15 with a separate test program of the same setup (all three wildcards gave `status=false listener_event=None`).

## Cause

`remote_queryable_matching_status` in `zenoh/src/net/routing/hat/broker/queries.rs` in 1.10.2:

```rust
let is_matching_res = |qabl: &Resource| {
    ...
    ke.includes(key_expr)          // whether the queryable's key includes the querier's key
};
let is_matching_info = |info: &QueryableInfoType| !complete || info.complete;
```

It requires the queryable's key `ke` to **include** the querier's key even when the querier does not ask for `complete`. `r/fam/one/status` does not include `r/fam/*/status`, so the result is false.

`get_matching_queryables` in `hat/p2p_peer/queries.rs` in 1.8.0 collected resources whose keys **intersect** (`ctx.matches`) and only checked inclusion when `complete` was set:

```rust
for mres in matches.iter() {
    if complete && !KeyExpr::keyexpr_include(mres.expr(), key_expr) {
        continue;
    }
    ...
}
```

Routing of a `get` is decided by intersection, so 1.10.2 ends up inconsistent: "matching is false, but `get` gets replies".

## Fixes

1. **Don't make queries conditional on wildcard matching.** Issue the `get` with a timeout, and if no reply comes, wait a little and retry. The replies themselves answer "who is there".
2. **Query concrete keys.** Matching is correct for a querier on a concrete key.
3. **Stay on 1.8.0.** 1.8.0 decides by intersection, so wildcards report true. However, 1.8.0 has the problems in [01](01-peer-join-declarations.md) and [02](02-expr-id-exhaustion.md), and [#2709](https://github.com/eclipse-zenoh/zenoh/pull/2709) (fail pushes to a closing transport immediately) is only in 1.10.2. Which to choose is a trade-off.

wildcard-query-retry shows 1 and 2, and for comparison also shows that "wait 2 seconds for matching, then query" gets nothing.
