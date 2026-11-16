# client-join

The fix for [peer-join-stall](../peer-join-stall): make the joining side a **client** (zenoh 1.8.0).

**Confirmed on:** zenoh 1.8.0 (2026-11-15) and 1.10.2 (2026-11-17), macOS. Other versions were not tried.

The program is the same as peer-join-stall, except that session B's mode is `"client"` (`JOIN_MODE`).

- A listens on the fixed endpoint `tcp/127.0.0.1:17511`
- B uses `mode: "client"` and `connect/endpoints: ["tcp/127.0.0.1:17511"]`
- Both use `scouting/multicast/enabled: false`

A client only receives declarations matching the interests it sent, so A sends no declarations when it joins. B gets a reply within a few ms and A's `put` does not stall. See [docs/01](../docs/01-peer-join-declarations.md) for the mechanism.

## Run

```sh
make run            # N = 40000
make run N=20000
```

## Output (2026-11-15, macOS, release)

```
[A] READY 40000 queryables declared in 22.18s
[B] joining as client via tcp/127.0.0.1:17511
[B] session open after 2.62ms
OK: client B got its first reply after 7.51ms with N=40000; A's put blocked for up to 0 s; A closed the link 0 time(s)
```

Joining as a peer with the same N=40000 gets no reply even after 30 seconds ([peer-join-stall](../peer-join-stall)).
Changing N makes little difference: in another run on the same day it was 3 ms for both 10 000 and 20 000, and 5 ms for 40 000.
