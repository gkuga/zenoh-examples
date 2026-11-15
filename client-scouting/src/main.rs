//! A client that finds its peer by scouting, or connects before the peer listens (zenoh 1.8.0).
//!
//! 1. A peer answers multicast scouting on a private group. A client scouts for it:
//!    a. with the default `scouting/multicast/autoconnect` (a client connects to routers only);
//!    b. with `{client: ["router", "peer"]}`.
//! 2. A client connects to an endpoint where a peer starts listening 1 s later:
//!    a. with the default `connect/timeout_ms` (0 for a client: give up at once);
//!    b. with `connect/timeout_ms: -1` (keep trying).
//!
//! Usage: client-scouting [MULTICAST_GROUP] [PORT]

use std::time::{Duration, Instant};

const KEY: &str = "demo/hello";

fn base(mode: &str) -> zenoh::Config {
    let mut c = zenoh::Config::default();
    c.insert_json5("mode", &format!("\"{mode}\"")).unwrap();
    c
}

/// Opens a client with `config` (within 10 s), then queries KEY once.
async fn try_client(label: &str, config: zenoh::Config) -> Option<Duration> {
    let start = Instant::now();
    let session = match tokio::time::timeout(Duration::from_secs(10), zenoh::open(config)).await {
        Ok(Ok(session)) => session,
        Ok(Err(e)) => {
            println!("{label}: open failed after {:.2?}: {e}", start.elapsed());
            return None;
        }
        Err(_) => {
            println!("{label}: open did not return within 10 s");
            return None;
        }
    };
    let opened = start.elapsed();
    let replies = session
        .get(KEY)
        .timeout(Duration::from_secs(2))
        .await
        .unwrap();
    let mut answered = 0;
    while let Ok(reply) = replies.recv_async().await {
        answered += reply.result().is_ok() as usize;
    }
    println!(
        "{label}: open after {opened:.2?}, {answered} reply from {KEY} after {:.2?}",
        start.elapsed()
    );
    // Keep the session: dropping it blocks for about 10 s.
    std::mem::forget(session);
    (answered > 0).then_some(opened)
}

async fn serve(session: &zenoh::Session) -> zenoh::query::Queryable<()> {
    let q = session.declare_queryable(KEY).callback(|q| {
        tokio::spawn(async move { q.reply(KEY, "hi").await.unwrap() });
    });
    q.await.unwrap()
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let group = std::env::args()
        .nth(1)
        .unwrap_or("224.0.0.224:17571".into());
    let port: u16 = std::env::args()
        .nth(2)
        .map_or(17572, |s| s.parse().unwrap());

    // 1. A peer that answers scouting on a private multicast group, listening on loopback.
    let mut peer = base("peer");
    peer.insert_json5("scouting/multicast/address", &format!("\"{group}\""))
        .unwrap();
    peer.insert_json5("listen/endpoints", "[\"tcp/127.0.0.1:0\"]")
        .unwrap();
    let peer = zenoh::open(peer).await.unwrap();
    let _q1 = serve(&peer).await;
    println!("peer answering scouting on {group}");

    let scouting_client = |autoconnect: Option<&str>| {
        let mut c = base("client");
        c.insert_json5("scouting/multicast/enabled", "true")
            .unwrap();
        c.insert_json5("scouting/multicast/address", &format!("\"{group}\""))
            .unwrap();
        if let Some(a) = autoconnect {
            c.insert_json5("scouting/multicast/autoconnect", a).unwrap();
        }
        c
    };
    let scout_default =
        try_client("1a. scouting, default autoconnect", scouting_client(None)).await;
    let scout_peer = try_client(
        "1b. scouting, autoconnect {client:[\"router\",\"peer\"]}",
        scouting_client(Some(r#"{client: ["router", "peer"]}"#)),
    )
    .await;

    // 2. A peer that starts listening on a fixed endpoint 1 s from now (no scouting).
    let endpoint = format!("tcp/127.0.0.1:{port}");
    let late = endpoint.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut c = base("peer");
        c.insert_json5("scouting/multicast/enabled", "false")
            .unwrap();
        c.insert_json5("listen/endpoints", &format!("[\"{late}\"]"))
            .unwrap();
        let session = zenoh::open(c).await.unwrap();
        let _q = serve(&session).await;
        println!("late peer listening on {late}");
        std::future::pending::<()>().await;
    });
    let connecting_client = |timeout_ms: Option<&str>| {
        let mut c = base("client");
        c.insert_json5("scouting/multicast/enabled", "false")
            .unwrap();
        c.insert_json5("connect/endpoints", &format!("[\"{endpoint}\"]"))
            .unwrap();
        if let Some(t) = timeout_ms {
            c.insert_json5("connect/timeout_ms", t).unwrap();
        }
        c
    };
    let connect_default = try_client(
        "2a. connect, default timeout_ms (0)",
        connecting_client(None),
    )
    .await;
    let connect_wait =
        try_client("2b. connect, timeout_ms -1", connecting_client(Some("-1"))).await;

    let fixed = scout_peer.is_some() && connect_wait.is_some();
    let failed = scout_default.is_none() && connect_default.is_none();
    if fixed && failed {
        println!(
            "OK: the defaults fail (no peer autoconnect; no wait for the endpoint) and both fixes connect \
             (scouted peer in {:.2?}, late endpoint in {:.2?})",
            scout_peer.unwrap(),
            connect_wait.unwrap()
        );
    } else {
        println!(
            "UNEXPECTED: default autoconnect ok={}, peer autoconnect ok={}, timeout 0 ok={}, timeout -1 ok={}",
            scout_default.is_some(),
            scout_peer.is_some(),
            connect_default.is_some(),
            connect_wait.is_some()
        );
    }
    // Closing or dropping a session blocks for about 10 s, so exit right away.
    std::process::exit(0);
}
