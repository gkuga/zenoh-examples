# client-scouting

client モードのセッションが peer に繋がらない 2 つの既定値と、その直し方（zenoh 1.8.0）。1 回の実行で失敗する設定と直した設定の両方を試す。

1. マルチキャストの scouting に応答する peer を、client が scouting で探す
   - a. `scouting/multicast/autoconnect` が既定値（client は router にしか繋がない）
   - b. `autoconnect: { client: ["router", "peer"] }`
2. 1 秒後に 127.0.0.1:17572 で listen し始める peer に、client が `connect/endpoints` で繋ぐ（scouting なし）
   - a. `connect/timeout_ms` が既定値（client は 0 = 1 回だけ試す）
   - b. `connect/timeout_ms: -1`（繋がるまで試し続ける）

繋がった client は `demo/hello` に `get` して、peer の queryable から応答が返ることを確かめる。

マルチキャストは既定の `224.0.0.224:7446` ではなく `224.0.0.224:17571` を使う（引数で変えられる: `cargo run --release -- 224.0.0.224:17581 17582`）。peer は 127.0.0.1 でだけ listen する。詳しくは [docs/04](../docs/04-client-scouting.md)。

## 実行

```sh
make run
```

## 出力（2026-11-15, macOS）

```
peer answering scouting on 224.0.0.224:17571
1a. scouting, default autoconnect: open failed after 3.00s: timeout at /Users/gkuga/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/zenoh-1.8.0/src/net/runtime/orchestrator.rs:1093.
1b. scouting, autoconnect {client:["router","peer"]}: open after 3.31ms, 1 reply from demo/hello after 3.83ms
2a. connect, default timeout_ms (0): open failed after 388.54µs: Unable to connect to any of [tcp/127.0.0.1:17572]!  at /Users/gkuga/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/zenoh-1.8.0/src/net/runtime/orchestrator.rs:374.
late peer listening on tcp/127.0.0.1:17572
2b. connect, timeout_ms -1: open after 1.01s, 1 reply from demo/hello after 1.01s
OK: the defaults fail (no peer autoconnect; no wait for the endpoint) and both fixes connect (scouted peer in 3.31ms, late endpoint in 1.01s)
```

- 1a は peer を見つけても繋がず、`scouting/timeout`（3 秒）で `zenoh::open` が失敗する。1b は数 ms で繋がる。
- 2a は相手がまだいないので即座に失敗する。2b は相手が listen した後の再試行で open が返る。再試行の間隔は `connect/retry` の既定値（1 s から 2 倍ずつ、最大 4 s）なので、1 s 目の再試行が相手の listen に間に合わないと 3 s になる（同じ日の別の実行で `late endpoint in 3.01s`）。
