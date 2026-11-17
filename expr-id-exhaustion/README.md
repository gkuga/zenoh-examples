# expr-id-exhaustion

A repro of a session's declarations never returning once the u16 ExprIds of one link are used up (zenoh 1.8.0, release build).

**Confirmed on:** zenoh 1.8.0 (2026-11-15) and 1.10.2 (2026-11-17), macOS. Other versions were not tried.

- Peer B (the parent process) first connects to session A (a child process) on `tcp/127.0.0.1:17541` (no multicast scouting).
- A then declares N = 70 000 liveliness tokens on distinct keys `demo/token/{i}`. Each declaration uses one ExprId on the face towards B.
- A watchdog thread in A reports any declaration that has not returned for 10 s or more. If one has not returned for 30 s, the parent prints `REPRODUCED`, kills the child and exits (overall limit 600 s).
- If all declarations return, it checks whether B can see the last token `demo/token/69999` with a liveliness `get`.

Declarations happen after the join, so the full push at join time from [peer-join-stall](../peer-join-stall) does not occur. Declarations are sent to B one by one.

See [docs/02](../docs/02-expr-id-exhaustion.md) for the mechanism and for why tokens are used instead of queryables (with queryables the session-side counter wraps first, and things silently break instead of hanging). A fix is described there.

## Run

```sh
make run              # N = 70000, release
```

A debug build (`cargo run -- 70000`) should panic on the overflow of `id += 1` in `get_next_local_id` instead of hanging (not tried here).

## Output (2026-11-15, macOS, release)

```
[B] connected as peer via tcp/127.0.0.1:17541
[A] B connected, declaring 70000 liveliness tokens
[A] declared 5000 at 363.3ms
[A] declared 10000 at 1.2s
[A] declared 15000 at 2.8s
[A] declared 20000 at 5.3s
[A] declared 25000 at 8.7s
[A] declared 30000 at 12.8s
[A] declared 35000 at 17.6s
[A] declared 40000 at 23.2s
[A] declared 45000 at 33.2s
[A] declared 50000 at 42.1s
[A] declared 55000 at 52.4s
[A] declared 60000 at 63.1s
[A] declared 65000 at 75.6s
[A] declaration #65537 has not returned for 10 s
[A] declaration #65537 has not returned for 20 s
[A] declaration #65537 has not returned for 30 s
REPRODUCED: A hung at declaration #65537 of 70000; it has not returned for 30s (65 535 ExprIds) (after 108.8s)
```

After all 65 535 numbers are used, the 65 536th declaration is given the reserved 0 (in release, `id += 1` wraps to 0), and at the **65 537th** the loop in `get_next_local_id` never ends. It keeps holding the routing table's write lock, so A can no longer do anything.
It was run 3 times on the same day and stalled at #65537 every time (100–317 s; the slow runs had another build running in parallel).
The time per declaration grows with N (linear search, so O(N²)).
