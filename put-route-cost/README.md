# put-route-cost

A repro of `put`s to undeclared keys getting slower the more declarations a session has (zenoh 1.8.0).

**Confirmed on:** zenoh 1.8.0 (2026-11-15) and 1.10.2 (2026-11-17), macOS. Other versions were not tried.

- Session A and peer B connect over `tcp/127.0.0.1:17581` (no multicast scouting). Declarations are made after connecting, so the join-time push from [peer-join-stall](../peer-join-stall) does not occur.
- For each of D keys `demo/thing/{i}/value`, A and B each declare one subscriber and one queryable (4 declarations per key, on concrete keys). D is increased through 0, 1 000, 5 000, 10 000, 20 000.
- For each D, A puts once to each of K = 10 000 distinct undeclared keys `demo/point/{j}/@state` and measures the time per put. They match no declaration, so the data goes nowhere; only route computation is measured.

An undeclared key has no resource, so its route is not cached, and every put intersects with all declared resources in `Resource::get_matches`. See [docs/05](../docs/05-put-route-cost.md) for the mechanism and [put-route-fix](../put-route-fix) for the fix.

## Run

```sh
make run
make run K=5000 D=0,2000,4000
```

## Output (2026-11-15, macOS, release)

```
K = 10000 puts to distinct undeclared keys, per D keys (4 declarations each)
      D      us/put       sweep
      0        0.35          4ms
   1000       24.44        244ms
   5000      138.45       1384ms
  10000      293.44       2934ms
  20000      813.87       8139ms
REPRODUCED: a put to an undeclared key took 0.35 us at D=0 and 813.87 us at D=20000 (2317x)
```

A single put grows from 0.35 µs at D = 0 to 814 µs at D = 20 000, roughly in proportion to D (more than proportionally at 20 000). Putting to 10 000 keys every 30 seconds would take 8 seconds per sweep.
The first run on the same day gave 1 015 µs at D = 20 000 (another build was running in parallel at the time).
