//! A querier on a wildcard never reports a match (zenoh 1.9.0 and later).
//!
//! Peer A declares queryables on `r/fam/one/status` and `r/fam/two/status`.
//! Peer B declares queriers on a concrete key and on three wildcards, and prints for each
//! its `matching_status()`, whether its `matching_listener` fired, and how many replies
//! its `get` received. On zenoh 1.10.2 the wildcard queriers report no match although
//! their `get` is answered; on 1.8.0 every querier matches.
//!
//! Usage: wildcard-querier-matching [PORT]

use std::time::Duration;

const QUERYABLES: [&str; 2] = ["r/fam/one/status", "r/fam/two/status"];
const QUERIERS: [&str; 4] = ["r/fam/one/status", "r/fam/*/status", "r/**", "r/fam/one/*"];

fn config(kind: &str, endpoint: &str) -> zenoh::Config {
    let mut c = zenoh::Config::default();
    c.insert_json5("mode", "\"peer\"").unwrap();
    // No multicast scouting: the sessions only meet through the endpoint below.
    c.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    if kind == "connect" {
        c.insert_json5("listen/endpoints", "[]").unwrap();
    }
    c.insert_json5(&format!("{kind}/endpoints"), &format!("[\"{endpoint}\"]"))
        .unwrap();
    c
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .map_or(17551, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");
    println!("zenoh {}", zenoh::GIT_VERSION);

    let a = zenoh::open(config("listen", &endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", &endpoint)).await.unwrap();
    while b.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let mut queryables = Vec::new();
    for key in QUERYABLES {
        let q = a.declare_queryable(key).callback(move |q| {
            tokio::spawn(async move { q.reply(key, "v").await.unwrap() });
        });
        queryables.push(q.await.unwrap());
    }
    // Give the declarations time to reach B.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let mut wrong = Vec::new();
    for key in QUERIERS {
        let querier = b
            .declare_querier(key)
            .timeout(Duration::from_secs(2))
            .await
            .unwrap();
        let listener = querier.matching_listener().await.unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), listener.recv_async()).await;
        let event = event.ok().and_then(|r| r.ok()).map(|s| s.matching());
        let status = querier.matching_status().await.unwrap().matching();
        let replies = querier.get().await.unwrap();
        let mut answered = 0;
        while let Ok(reply) = replies.recv_async().await {
            answered += reply.result().is_ok() as usize;
        }
        println!(
            "querier {key:<18} matching_status={status:<5} listener_event={:<11} get replies={answered}",
            format!("{event:?}")
        );
        if answered > 0 && !status {
            wrong.push(key);
        }
    }
    if wrong.is_empty() {
        println!(
            "OK: every querier that gets replies reports a match ({})",
            zenoh::GIT_VERSION
        );
    } else {
        println!(
            "REPRODUCED: {} querier(s) get replies but report no match: {wrong:?} ({})",
            wrong.len(),
            zenoh::GIT_VERSION
        );
    }
    // Closing or dropping a session blocks for about 10 s, so exit right away.
    std::process::exit(0);
}
