# zenoh-examples

[Zenoh](https://zenoh.io/) でハマった問題を最小構成で再現するサンプルと、その解決方法を示すサンプル集。Rust で書いている。
各サンプルは独立した Cargo プロジェクトで（サンプルごとに zenoh のバージョンを固定するので、空の `[workspace]` で親から切り離している）、ディレクトリ内で `make run` すれば動く。

仕組みの解説は [docs](docs/README.md) にまとめている。

| ディレクトリ | 種類 | zenoh | 内容 |
| --- | --- | --- | --- |
| [peer-join-stall](peer-join-stall) | 再現 | 1.8.0 | 宣言を大量に持つセッションに **peer** が参加すると、最初の応答が返らず、相手の `put` も止まり、リンクが 5 秒ごとに切れる |
| [client-join](client-join) | 解決 | 1.8.0 | 同じ構成で参加側を **client** にすると、4 万 queryable でも数 ms で応答する |
| [expr-id-exhaustion](expr-id-exhaustion) | 再現 | 1.8.0 | 1 本のリンクで u16 の ExprId (65 535 個) を使い切ると、宣言が二度と返らない (release ビルド) |
| [expr-id-exhaustion-fork](expr-id-exhaustion-fork) | 解決 (fork) | 1.8.0 + patch | `[patch.crates-io]` で fork (gkuga/zenoh `nue-1.8.0-expr-id-2`) を使うと、使い切っても止まらない |
| [wildcard-querier-matching](wildcard-querier-matching) | 再現 | 1.10.2 / 1.8.0 | ワイルドカードの querier の `matching_status()` が 1.9.0 以降 false のまま (`get` には応答がある)。`make compare` で 1.8.0 と比べられる |
| [wildcard-query-retry](wildcard-query-retry) | 解決 | 1.10.2 | matching を待たずに、タイムアウト付きの `get` を再試行する／具体的なキーで問い合わせる |
| [client-scouting](client-scouting) | 再現と解決 | 1.8.0 | client が scouting で見つけた **peer** に繋がらない (autoconnect)、まだ listen していない相手に即失敗する (`connect/timeout_ms`) |
| [put-route-cost](put-route-cost) | 再現 | 1.8.0 | 宣言されていないキーへの `put` は経路がキャッシュされず、毎回すべての宣言と交差を取る。宣言 2 万キーで 1 回 0.35 µs → 814 µs |
| [put-route-fix](put-route-fix) | 解決 | 1.8.0 | (a) キーごとの `Publisher` を通して put（1 回 0.1 µs のまま）、(b) 多数の値を 1 回の put にまとめる |

## 共通の作り

- どのサンプルも `main.rs` 1 つで、起きたことを表示して自分で終了し、最後に `REPRODUCED: ...` や `OK: ...` の判定行を出す。
- Zenoh のセッションを close / drop すると 10 秒ほど待たされるので、最後は `std::process::exit(0)` で抜けている。
- 通信は 127.0.0.1 の TCP だけ。ポートはサンプルごとに固定 (17501〜17582) なので、同時に動かしてもぶつからない。scouting を扱う client-scouting 以外はマルチキャスト scouting を切っている。
- 時間を測るサンプルがあるので、`make run` は `cargo run --release`。最初のビルドは 1〜2 分かかる。
- 計測値は 2026-11-15 に macOS (Apple M3 Pro, 11 コア, Rust 1.96.0) で取ったもの。

## 背景

エッジ向けのアプリケーションで、計測ポイントごとに Zenoh の queryable や subscriber を宣言していたところ、数千ポイントでデーモンが固まった。その調査で見つかった問題をここに切り出した。
