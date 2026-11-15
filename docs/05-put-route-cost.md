# Cost of a put to an undeclared key

Samples: [put-route-cost](../put-route-cost) (repro), [put-route-fix](../put-route-fix) (fix)

## What happens

Profiling a daemon on 2026-11-15 showed about a third of its CPU spent in `Resource::get_matches` → `zenoh_keyexpr ... Intersector::intersect` under `Session::put`.
Every 30 seconds the daemon put to N distinct, **undeclared** keys (`<key>/@state`), while a peer connected to the same session declared about two subscribers and two queryables per key on concrete keys.

## Mechanism (zenoh 1.8.0)

`get_data_route` in `zenoh/src/net/routing/dispatcher/pubsub.rs`:

```rust
let compute_route = || hat_code.compute_data_route(tables, expr, local_context, face.whatami);
if let Some(data_routes) = expr
    .resource()
    .as_ref()
    .and_then(|res| res.context.as_ref())
    .map(|ctx| &ctx.data_routes)
{
    return get_or_set_route(data_routes, tables.routes_version, ...);
}
compute_route()
```

- If the key has a resource, the route is cached in that resource's `data_routes` and only recomputed when `routes_version` changes.
- An undeclared key has no resource, so `compute_route` runs on every put. For a key without a resource, the p2p `compute_data_route` intersects the key expression with every declared resource via `Resource::get_matches`. So the cost of one put is proportional to the number of declarations D, and one sweep over N keys costs N × D.

## Fixes

- **(a) Declare a `Publisher` once per key and put through it.** The declaration creates a resource, so the route gets cached. This works on 1.8.0 too (in put-route-fix the cost per put does not change as D grows from 0 to 20 000). But a publisher is also a declaration, so by the counting in [01](01-peer-join-declarations.md) / [02](02-expr-id-exhaustion.md) it adds one declaration per key.
- **(b) Batch many values into one put.** The intersection is computed for one put only. That one put is still proportional to D, but K puts per sweep become one. No declarations are added.

Measurements are in each sample's README.
