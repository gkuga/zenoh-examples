# expr-id-exhaustion-fork

[expr-id-exhaustion](../expr-id-exhaustion) を fork の修正で動かす（zenoh 1.8.0 + patch）。

プログラムは expr-id-exhaustion と同じ（ポートだけ `17542`）。違いは Cargo.toml の次の行だけ:

```toml
[patch.crates-io]
zenoh = { git = "https://github.com/gkuga/zenoh", tag = "nue-1.8.0-expr-id-2" }
```

タグ `nue-1.8.0-expr-id-2` は commit `25c6174e6477c355c4db6fd09e2044523fb73b37` で、1.8.0 に次の 2 つを足したもの:

- ExprId をラップするカーソルで割り当てる（償却 O(1)。undeclare で解放された番号を優先して再利用）
- 全部使用中なら番号を振らずに完全なキー式を送り、face ごとに 1 回だけ警告する（`No expression id left, sending ... and later key expressions in full`）

zenoh はリポジトリ内の他の `zenoh-*` に path で依存しているので、`Cargo.lock` の `zenoh-*` はすべてこの commit から来る。初回のビルドで GitHub から clone する。詳しくは [docs/02](../docs/02-expr-id-exhaustion.md)。

## 実行

```sh
make run              # N = 70000、release
```

## 出力（2026-11-15, macOS, release）

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

65 536 個目（`demo/token/65535`）で番号が尽きて警告が 1 回出て、残りは完全なキーで送られる。70 000 個すべての宣言が返り、B は最後のトークンを見られる。

### 同じ日の他の実行

この最終版の前に 3 回実行し、2 回は `FAILED` だった。どちらも修正そのものの失敗ではないと考えているが、記録しておく。

1. `OK`。このときは B の確認が 1 回だけ（タイムアウト 5 s）だった。
2. `FAILED: ... B does not see demo/token/69999`。確認が 1 回 5 s だけで、B がまだ 70 000 個の宣言を取り込み中だったとみられる。次の実行では B が見られたのは DONE の 8.1 s 後だった。そのため確認を「60 s まで 1 s おきに再試行」に変えた。
3. `OK`（DONE の 8.1 s 後）。
4. `FAILED: ... within 60 s`。ログの時刻が 15 分飛んでおり、実行中に Mac がスリープした。復帰後にリンクが切れ（`Closing transport!`）、B が繋ぎ直すと 70 000 個を持つ A への peer の参加になり、[peer-join-stall](../peer-join-stall) と同じく 5 秒ごとにリンクが閉じ続けた。上の出力はスリープを防いで（`caffeinate -i`）実行し直したもの。
