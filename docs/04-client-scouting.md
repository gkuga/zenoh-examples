# client と scouting / connect

サンプル: [client-scouting](../client-scouting)（再現と解決を 1 つで）

client モードのセッションは、peer とは既定値がいくつか違う。peer から client に切り替えるとき（[01](01-peer-join-declarations.md)）にハマる 2 点。

## 1. scouting で見つけた peer に繋がらない

`scouting/multicast/autoconnect` の既定値（`zenoh-config/src/defaults.rs`）:

| モード | 既定値 | 意味 |
| --- | --- | --- |
| router | `""` | 自動では繋がない |
| peer | `"router\|peer"` | router にも peer にも繋ぐ |
| client | `"router"` | **router にしか繋がない** |

client はマルチキャストの scouting で peer を見つけても繋がず、`scouting/timeout`（3 秒）で `zenoh::open` が失敗する。peer にも繋ぐには:

```json5
scouting: { multicast: { autoconnect: { client: ["router", "peer"] } } }
```

計測（2026-11-15）: 既定値では 3.00 s で失敗、上の設定では 3.3 ms で open し、応答も返った（別の機会の計測では 12 ms）。

## 2. まだ listen していない相手に即失敗する

`connect/timeout_ms` の既定値は router と peer が `-1`（繋がるまで試し続ける）、client が **`0`**（1 回試して失敗したら `zenoh::open` がエラー）。`connect/exit_on_failure` も client だけ `true`。
相手より先に client を起動すると、`Unable to connect to any of [tcp/127.0.0.1:...]!` で即座に失敗する。

```json5
connect: { endpoints: ["tcp/127.0.0.1:17572"], timeout_ms: -1 }
```

にすると、相手が listen するまで open が待つ。

計測（2026-11-15）: 1 s 後に listen する相手に対し、既定値では 0.4 ms で失敗、`-1` では 1.01 s で open した（再試行は `connect/retry` の既定で 1 s から 2 倍ずつ、最大 4 s なので、1 s 目に間に合わないと 3 s になる）。

## マルチキャストを手元に閉じる

client-scouting は既定のグループ `224.0.0.224:7446` ではなく `224.0.0.224:17571` を使い、peer は 127.0.0.1 でだけ listen する。TTL は既定の 1 なのでルータは越えない。
macOS で `scouting/multicast/interface` を `"lo0"` にすると scouting が成立しなかった（client が peer を見つけられない）ので、インタフェースは既定の `"auto"` のまま。scouting のパケット自体は LAN に出るが、そのポートを聞いているのはこのサンプルだけ。
