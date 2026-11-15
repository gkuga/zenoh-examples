# peer-join-stall

A repro of things stalling when a **peer** joins a session that holds many declarations (zenoh 1.8.0).

- Session A (a child process) declares N queryables on `demo/decl/{i}`, plus one queryable on `demo/target` that replies. After declaring, it keeps putting to `demo/tick` every 100 ms, and a watchdog thread reports any put that has not returned for 1 second or more.
- Session B (the parent process) connects as a **peer** to `tcp/127.0.0.1:17501` and repeats a `get` on `demo/target` (2 s timeout). It measures the time until the first reply, within 30 s.
- Multicast scouting is disabled. A runs in a separate process because in the same process it would share the zenoh runtime's threads.

In p2p, all declarations are sent to a joining peer while holding the routing table's write lock. See [docs/01](../docs/01-peer-join-declarations.md) for the mechanism and [client-join](../client-join) for the fix.

## Run

```sh
make run            # N = 40000
make run N=20000
```

## Output (2026-11-15, macOS, release)

```
[A] READY 40000 queryables declared in 23.62s
[B] joining as peer via tcp/127.0.0.1:17501
[B] session open after 501.70ms
[A] put on demo/tick blocked for 1 s so far
[A] put on demo/tick blocked for 2 s so far
[B] no reply yet at 2.50s
[A] put on demo/tick blocked for 3 s so far
[A] put on demo/tick blocked for 4 s so far
[B] no reply yet at 4.61s
[A] put on demo/tick blocked for 5 s so far
[A] 2026-11-15T09:05:05.230857Z ERROR acc-0 ThreadId(16) zenoh_transport::unicast::universal::tx: Unable to push non droppable network message to 44e0537e904e7afbb38ef9cc8fc8b5be. Closing transport!
[A] put on demo/tick blocked for 6 s so far
...
REPRODUCED: peer B got no reply within 30s with N=40000; A's put blocked for up to 30 s; A closed the link 6 time(s)
```

- B gets no reply even after waiting 30 seconds.
- A's `put` does not return for 30 seconds from right after B joins (`put ... blocked for N s so far` in the log).
- A closes the link every 5 seconds with `Unable to push non droppable network message ... Closing transport!`, and the same thing happens each time B reconnects.

Results for different N (same day, same machine):

| N | B's first reply | A's put stall | Times A closed the link |
| --- | --- | --- | --- |
| 1 000 | 0.50 s | none | 0 |
| 10 000 | 1.01 s | none | 0 |
| 20 000 | 5.40 s | none | 0 |
| 30 000 | none within 30 s | no stall of 1 s or more | 6 |
| 40 000 | none within 30 s | 30 s | 6 |

The 0.5 s is the time a peer's `zenoh::open` takes to return after waiting for `scouting/delay` (500 ms); anything slower than that is due to the push of declarations.
On this Mac it started to stall between 20 000 and 30 000. In another environment (an arm64 edge device) it stalled at a smaller N: about 11 s at 10 000, and no reply within 15 s at 20 000.

The time for A to declare the N queryables itself grows with N² (about 22 s at 40 000). See the end of [docs/01](../docs/01-peer-join-declarations.md).
