# expr-id-exhaustion-fork

Runs [expr-id-exhaustion](../expr-id-exhaustion) with the fix from a fork (zenoh 1.8.0 + patch).

**Confirmed on:** zenoh 1.8.0 with the fork patch (2026-11-15), macOS.

The program is the same as expr-id-exhaustion (only the port is `17542`). The only difference is these lines in Cargo.toml:

```toml
[patch.crates-io]
zenoh = { git = "https://github.com/gkuga/zenoh", rev = "25c6174e6477c355c4db6fd09e2044523fb73b37" }
```

The fix is commit `25c6174e6477c355c4db6fd09e2044523fb73b37`, which is 1.8.0 plus these two changes:

- ExprIds are allocated by a wrapping cursor (amortized O(1); numbers freed by undeclare are reused first)
- When every number is in use, the full key expression is sent without a number, with a warning logged once per face (`No expression id left, sending ... and later key expressions in full`)

zenoh depends on the other `zenoh-*` crates in its repository by path, so every `zenoh-*` in `Cargo.lock` comes from this commit. The first build clones it from GitHub. See [docs/02](../docs/02-expr-id-exhaustion.md) for details.

## Run

```sh
make run              # N = 70000, release
```

## Output (2026-11-15, macOS, release)

```
[B] connected as peer via tcp/127.0.0.1:17542
[A] B connected, declaring 70000 liveliness tokens
[A] declared 5000 at 323.7ms
[A] declared 10000 at 1.2s
[A] declared 15000 at 2.8s
[A] declared 20000 at 5.6s
[A] declared 25000 at 11.3s
[A] declared 30000 at 16.4s
[A] declared 35000 at 23.5s
[A] declared 40000 at 33.3s
[A] declared 45000 at 47.4s
[A] declared 50000 at 58.7s
[A] declared 55000 at 80.0s
[A] declared 60000 at 95.1s
[A] declared 65000 at 116.2s
[A] 2026-11-15T09:35:04.502345Z  WARN main ThreadId(01) zenoh::net::routing::dispatcher::resource: Face{1, 50746b0aa2c51bc65c9969dde3fe0fc6} No expression id left, sending demo/token/65535 and later key expressions in full
[A] declared 70000 at 129.6s
[A] DONE all 70000 declared in 129.6s
OK: A declared all 70000 tokens and B sees demo/token/69999 (2.2s after DONE) (after 132.8s)
```

At the 65 536th token (`demo/token/65535`) the numbers run out, the warning is logged once, and the rest are sent with full keys. All 70 000 declarations return, and B can see the last token.

### Other runs on the same day

Before this final version there were 3 runs, 2 of which were `FAILED`. I don't believe either was a failure of the fix itself, but they are recorded here.

1. `OK`. At this point B checked only once (5 s timeout).
2. `FAILED: ... B does not see demo/token/69999`. There was only one 5 s check, and B was apparently still ingesting the 70 000 declarations. In the next run B saw it 8.1 s after DONE, so the check was changed to "retry every 1 s for up to 60 s".
3. `OK` (8.1 s after DONE).
4. `FAILED: ... within 60 s`. The log timestamps jumped by 15 minutes: the Mac went to sleep during the run. After waking, the link dropped (`Closing transport!`), and B's reconnect became a peer joining an A with 70 000 declarations, so the link kept being closed every 5 seconds just like [peer-join-stall](../peer-join-stall). The output above is a rerun with sleep prevented (`caffeinate -i`).
