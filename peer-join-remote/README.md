# peer-join-remote

How long a `put` waits while a **peer** joins a session that holds many declarations, with the two
sessions on two hosts. It is [peer-join-stall](../peer-join-stall) split into two commands, with a
histogram of every put's duration instead of a 1 s watchdog only.

**Confirmed on:** zenoh 1.10.2 with patches to the join push (2026-11-20), two arm64 Linux devices on
one LAN, and zenoh 1.10.2 (2026-11-20) over loopback on x86_64 Linux. Other versions were not tried.

- Session A (`serve N ENDPOINT`) listens on ENDPOINT, declares N queryables on `demo/decl/{i}` plus
  one on `demo/target` that replies, prints `READY` with its RSS, then puts to `demo/tick` every 10 ms.
  - Every put's duration goes into a histogram with 1 ms buckets. Once a second while puts are slow
    (one of 10 ms or more ended in the last second, or one has been running that long), and on exit
    (SIGTERM, SIGINT or stdin closed), A prints the count, p50, p99, p99.9, max, and how many puts
    took 10, 50, 100, 500 and 1000 ms or more.
  - A thread polls A's peers every 10 ms to note when B connects. A then prints the longest put
    wait between that and its first reply to B (`join: ...`). A put still running counts if it
    started before the reply.
  - A watchdog thread prints `put on demo/tick blocked for N s so far` for a put that hasn't
    returned within 1 s, as in peer-join-stall.
- Session B (`N [DEADLINE_S [LINGER_S]]`) connects as a **peer** to the endpoint in `PJS_REMOTE` and
  repeats a `get` on `demo/target` (2 s timeout). It prints the time to the first reply, within
  DEADLINE_S (default 30). Then it stays connected for LINGER_S (default 10), so that A finishes
  pushing its declarations to B, and exits.
- Multicast scouting is disabled. The sessions only meet through the endpoint.

See [docs/01](../docs/01-peer-join-declarations.md) for why a joining peer makes A's puts wait.

## Run

On host A (the address is one B can reach):

```sh
make serve N=200000 ENDPOINT=tcp/<A's address>:17501
```

On host B, once A has printed `READY`:

```sh
PJS_REMOTE=tcp/<A's address>:17501 make join N=200000 DEADLINE=120 LINGER=20
```

Stop A with Ctrl-C (or SIGTERM) to get the final statistics. A also exits when its stdin closes, so
when it runs in the background, give it a stdin that stays open (e.g. a FIFO held open by another
process).

### Building against another zenoh tree, or for another host

`make build-variant` copies this project to `BUILD_DIR/VARIANT` (default `~/pjr-build`), replaces the
`zenoh` dependency there with a git dependency, and builds it. `Cargo.toml` here is left unchanged:

```sh
make build-variant ZENOH_GIT=<git url of a zenoh tree> REV=<commit> VARIANT=<name>
```

By default it builds for arm64 Linux with glibc 2.31 using
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)
(`rustup target add aarch64-unknown-linux-gnu`, and zig). `TARGET=` builds for the host with plain
`cargo`. The binary is `BUILD_DIR/VARIANT/peer-join-remote`.

## Output (2026-11-20, arm64 devices, N = 500 000, the chunked-push tree below)

A:

```
READY 500000 queryables declared in 9.05s; rss 860 MB
B connected at 13.291 s
put stats: count=411 p50=0ms p99=4ms p99.9=155ms max=155ms >=10ms=2 >=50ms=1 >=100ms=1 >=500ms=0 >=1000ms=0
join: first reply to B 0.818 s after B connected; max put wait in between: 155 ms
put stats: count=651 p50=0ms p99=8ms p99.9=155ms max=155ms >=10ms=5 >=50ms=1 >=100ms=1 >=500ms=0 >=1000ms=0
...
put stats (one put running for 114 ms): count=2166 p50=0ms p99=4ms p99.9=11ms max=155ms >=10ms=7 >=50ms=1 >=100ms=1 >=500ms=0 >=1000ms=0
put stats: count=2218 p50=0ms p99=4ms p99.9=11ms max=513ms >=10ms=8 >=50ms=2 >=100ms=2 >=500ms=1 >=1000ms=0
put stats (final): count=2352 p50=0ms p99=4ms p99.9=11ms max=513ms >=10ms=8 >=50ms=2 >=100ms=2 >=500ms=1 >=1000ms=0
join: first reply to B 0.818 s after B connected; max put wait in between: 155 ms
rss 906 MB, peak rss 1043 MB
```

B:

```
[B] joining as peer via tcp/<A's address>:17501
[B] session open after 504.30ms
[B] first reply after 1.48s with N=500000
[B] rss 101 MB at first reply
[B] rss 656 MB, peak rss 656 MB after staying 20s
```

- The 155 ms put is the join. The 513 ms put near the end comes when B exits after its 20 s stay
  (by put count, about 20 s after the first reply): A removing B's face also makes a put wait.
- `p50=0ms` means under 1 ms.

## Results (2026-11-20, two arm64 Linux devices, 4 cores and 2 GB RAM each)

Three zenoh 1.10.2 trees with patches:

- **base**: patches unrelated to the join push.
- **ordered**: base + the push of declarations to a joining face is sent before any later message
  to that face.
- **chunked**: ordered + the push is collected under the tables' lock once and then sent in
  chunks, releasing the lock between chunks.

B stays 20 s after its first reply. "Join max" is A's longest put between B's connection and the
first reply. "Max" and "≥100 ms" cover the whole run (about 30 s, including B leaving).

| tree | N | B's first reply | join max | p99 | max | puts ≥100 ms | link closes | A's RSS after READY |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| base | 200 000 | 3.34 s | 899 ms | 1 ms | 899 ms | 2 | 0 | 347 MB |
| ordered | 200 000 | 4.26 s | 1058 ms | 1 ms | 1058 ms | 2 | 0 | 348 MB |
| chunked | 200 000 | 0.98 s | 63 ms | 4 ms | 275 ms | 1 | 0 | 347 MB |
| base | 500 000 | 9.76 s | 2458 ms | 1 ms | 2458 ms | 2 | 0 | 861 MB |
| ordered | 500 000 | 10.59 s | 2541 ms | 1 ms | 2541 ms | 2 | 0 | 860 MB |
| chunked | 500 000 | 1.43-1.48 s (3 runs) | 149-155 ms | 3-4 ms | 513-516 ms | 2 | 0 | 860 MB |

- With the chunked push, the join's longest put drops from about 0.9-2.5 s to about 60-150 ms, and B
  gets its first reply 3-7 times sooner. That longest put still grows with N.
- In every tree, B leaving costs one more long put (the second of the "≥100 ms" puts; the only
  one above the join max in the chunked rows).
- N = 1 000 000 was not run on the devices: A alone needs about 1.7 GB after READY, 2.0 GB at peak
  (measured on x86_64), more than the devices have free.
