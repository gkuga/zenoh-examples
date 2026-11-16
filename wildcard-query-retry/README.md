# wildcard-query-retry

The fix for [wildcard-querier-matching](../wildcard-querier-matching): don't make queries conditional on a wildcard's `matching_status()` (zenoh 1.10.2).

**Confirmed on:** zenoh 1.10.2 (2026-11-15), macOS; also built against 1.8.0, where the gated way works too (see below).

Peer A declares its queryables 300 ms after B (standing in for a service that comes up later). B wants everything under `r/fam/*/status`.

1. **retry**: issue a `get` with a 500 ms timeout; if there are no replies, wait 200 ms and retry (up to 5 times). Matching is not consulted.
2. **concrete**: a querier per concrete key. For these, `matching_status()` is correct.
3. **gated (the way not to do it)**: wait up to 2 seconds for the wildcard querier to report a match, then query. Even though A came up long before, no match is reported, so it never queries.

TCP on 127.0.0.1:17561, no multicast scouting.

Another fix is to stay on 1.8.0, where even approach 3 works (checked on 2026-11-15 by building the same `main.rs` against 1.8.0: `3. gated on matching: 2 replies`, verdict `OK: all three got the replies; the gate works on this version (v1.8.0-release)`; this repository does not include a manifest for 1.8.0). However, 1.8.0 has the problems in [peer-join-stall](../peer-join-stall) and [expr-id-exhaustion](../expr-id-exhaustion), and the fix that makes pushes to a closing transport fail immediately ([#2709](https://github.com/eclipse-zenoh/zenoh/pull/2709)) is only in 1.10.2. See [docs/03](../docs/03-wildcard-querier-matching.md) for details.

## Run

```sh
make run
```

## Output (2026-11-15, macOS)

```
zenoh v1.10.2-release
1. retry attempt 1 at 264.50µs: 0 replies
1. retry attempt 2 at 201.89ms: 0 replies
1. retry attempt 3 at 404.67ms: 2 replies
2. concrete r/fam/one/status: matching=true replies=1
2. concrete r/fam/two/status: matching=true replies=1
3. gated on matching: 0 replies []
OK: the gated query got nothing, but retrying the wildcard got 2 and concrete keys got 2 (v1.10.2-release)
```

While A is not there yet, a `get` finishes immediately with 0 replies (it does not wait for the timeout), and on the 3rd attempt, after A has declared, both replies come back.
