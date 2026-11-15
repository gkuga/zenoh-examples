# put-route-fix

[put-route-cost](../put-route-cost) の解決方法 2 つ（zenoh 1.8.0）。

構成は put-route-cost と同じ（`tcp/127.0.0.1:17582`）。各 D で、A が K = 10 000 個の値（キー `demo/point/{j}/@state` ごとに 1 つ）を次の 3 通りで出し、値 1 つあたりの時間を比べる。

| 列 | 書き方 |
| --- | --- |
| undeclared | `session.put(key, ..)` をキーごとに（問題の書き方、比較用） |
| pub round1 / round2 | **(a)** 最初に（D の宣言より前に）キーごとの `Publisher` を宣言しておき、それを通して put。各 D で 2 巡する（round1 は新しい宣言の直後） |
| aggregated | **(b)** K 個の値を 1 つのペイロード（`key=value` の行）にまとめ、`demo/points/@state` に 1 回だけ put |

## 実行

```sh
make run
```

## 出力（2026-11-15, macOS, release）

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

- **(a) publisher は 1.8.0 でも効く。** D を 0 から 20 000 に増やしても 1 回 0.1 µs のまま変わらない。publisher の宣言でキーにリソースができ、経路がその `data_routes` にキャッシュされるため（`routes_version` が変わると計算し直すが、リソースが持つ `matches` を使うので安い。round1 も round2 と同じ）。代わりに、publisher も宣言なのでキーあたり 1 宣言増え、10 000 個の宣言に 1.65 s かかった。
- **(b) まとめた put** は一巡 0.45 ms（D = 0）から 2.51 ms（D = 20 000）。この 1 回の put の交差計算はまだ D に比例して伸びるが、一巡あたり 10 000 回が 1 回になる。宣言は増えない。
- undeclared は put-route-cost と同じく D = 20 000 で 783 µs（一巡 7.8 s）。

publisher の put が速いのは、どの subscriber にも一致せず送り先がないことも効いている。一致する subscriber があれば、その分の送信のコストが加わる（ここでは測っていない）。
