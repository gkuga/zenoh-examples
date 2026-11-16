# put-route-fix

Two fixes for [put-route-cost](../put-route-cost) (zenoh 1.8.0).

**Confirmed on:** zenoh 1.8.0 (2026-11-15) and 1.10.2 (2026-11-17), macOS. Other versions were not tried.

The setup is the same as put-route-cost (`tcp/127.0.0.1:17582`). For each D, A emits K = 10 000 values (one per key `demo/point/{j}/@state`) in the following three ways and compares the time per value.

| Column | How |
| --- | --- |
| undeclared | `session.put(key, ..)` per key (the problematic way, for comparison) |
| pub round1 / round2 | **(a)** Declare a `Publisher` per key up front (before the D declarations) and put through it. Two sweeps per D (round1 runs right after the new declarations) |
| aggregated | **(b)** Pack the K values into one payload (`key=value` lines) and put once to `demo/points/@state` |

## Run

```sh
make run
```

## Output (2026-11-15, macOS, release)

```
declared 10000 publishers in 1.65s
us per value, K = 10000 values, per D keys (4 declarations each):
      D   undeclared   pub round1   pub round2   aggregated
      0         0.35         0.12         0.10        0.045
   1000        24.52         0.11         0.09        0.052
   5000       134.93         0.11         0.10        0.063
  10000       278.81         0.11         0.09        0.091
  20000       783.02         0.12         0.10        0.251
OK: at D=20000 an undeclared put costs 783.02 us (7830 ms per sweep of 10000); (a) a publisher put 0.10 us (at D=0: 0.10 us); (b) the aggregated put 2.51 ms per sweep (at D=0: 0.45 ms)
```

- **(a) Publishers work even on 1.8.0.** The cost stays at 0.1 µs per put as D goes from 0 to 20 000. Declaring the publisher creates a resource for the key, and the route is cached in its `data_routes` (it is recomputed when `routes_version` changes, but that uses the resource's `matches` and is cheap; round1 is the same as round2). The downside is that a publisher is also a declaration, adding one declaration per key; declaring 10 000 of them took 1.65 s.
- **(b) The aggregated put** costs 0.45 ms per sweep (D = 0) to 2.51 ms (D = 20 000). The intersection for that single put still grows with D, but 10 000 puts per sweep become one. No declarations are added.
- undeclared is 783 µs at D = 20 000 (7.8 s per sweep), the same as in put-route-cost.

The publisher puts are also fast because they match no subscriber and have nowhere to go. With a matching subscriber, the cost of sending would be added (not measured here).
