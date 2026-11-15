# client-join

[peer-join-stall](../peer-join-stall) の解決方法: 参加する側を **client** にする（zenoh 1.8.0）。

プログラムは peer-join-stall と同じで、セッション B のモードが `"client"` なだけ（`JOIN_MODE`）。

- A は固定のエンドポイント `tcp/127.0.0.1:17511` で listen する
- B は `mode: "client"`、`connect/endpoints: ["tcp/127.0.0.1:17511"]`
- どちらも `scouting/multicast/enabled: false`

client は自分が送った interest に合う宣言しか受け取らないので、参加しても A は宣言を送らない。B は数 ms で応答を得て、A の `put` も止まらない。仕組みは [docs/01](../docs/01-peer-join-declarations.md)。

## 実行

```sh
make run            # N = 40000
make run N=20000
```

## 出力（2026-11-15, macOS, release）

```
[A] READY 40000 queryables declared in 22.18s
[B] joining as client via tcp/127.0.0.1:17511
[B] session open after 2.62ms
OK: client B got its first reply after 7.51ms with N=40000; A's put blocked for up to 0 s; A closed the link 0 time(s)
```

同じ N=40000 で peer として参加すると、30 秒待っても応答がない（[peer-join-stall](../peer-join-stall)）。
N を変えてもほとんど変わらず、同じ日の別の実行では 10 000 と 20 000 でどちらも 3 ms、40 000 で 5 ms だった。
