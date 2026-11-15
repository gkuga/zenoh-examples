//! Querying a wildcard without trusting `matching_status()` (zenoh 1.10.2).
//!
//! Peer A declares its queryables 300 ms after peer B starts, as a service that
//! comes up late would. B wants every `r/fam/*/status`:
//! 1. retry: query with a timeout and retry until something answers. No match needed.
//! 2. concrete: a querier per concrete key; its matching status works.
//! 3. gated (the pattern to avoid): wait up to 2 s for the wildcard querier to report
//!    a match, then query. On 1.9.0 and later the match never comes, so B never queries.
//!
//! Usage: wildcard-query-retry [PORT]

use std::time::{Duration, Instant};

use zenoh::query::Querier;

const QUERYABLES: [&str; 2] = ["r/fam/one/status", "r/fam/two/status"];
const WILDCARD: &str = "r/fam/*/status";

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

/// Sends one query and returns the keys that answered.
async fn query(querier: &Querier<'_>) -> Vec<String> {
    let replies = querier.get().await.unwrap();
    let mut keys = Vec::new();
    while let Ok(reply) = replies.recv_async().await {
        if let Ok(sample) = reply.result() {
            keys.push(sample.key_expr().to_string());
        }
    }
    keys
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let port: u16 = std::env::args()
        .nth(1)
        .map_or(17561, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");
    println!("zenoh {}", zenoh::GIT_VERSION);

    let a = zenoh::open(config("listen", &endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", &endpoint)).await.unwrap();
    while b.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // A's service comes up a little later than B.
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut queryables = Vec::new();
        for key in QUERYABLES {
            let q = a.declare_queryable(key).callback(move |q| {
                tokio::spawn(async move { q.reply(key, "v").await.unwrap() });
            });
            queryables.push(q.await.unwrap());
        }
        std::future::pending::<()>().await;
    });
    let start = Instant::now();

    let wildcard = b
        .declare_querier(WILDCARD)
        .timeout(Duration::from_millis(500))
        .await
        .unwrap();
    // 1. Query with a timeout and retry; the replies themselves say who is there.
    let mut retried = Vec::new();
    for attempt in 1..=5 {
        retried = query(&wildcard).await;
        println!(
            "1. retry attempt {attempt} at {:.2?}: {} replies",
            start.elapsed(),
            retried.len()
        );
        if !retried.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    // 2. Concrete keys: a querier per key, whose matching status is reliable.
    let mut concrete = Vec::new();
    for key in QUERYABLES {
        let querier = b.declare_querier(key).await.unwrap();
        let matching = querier.matching_status().await.unwrap().matching();
        let keys = query(&querier).await;
        println!(
            "2. concrete {key}: matching={matching} replies={}",
            keys.len()
        );
        if matching {
            concrete.extend(keys);
        }
    }

    // 3. Gated on the wildcard querier's matching status (the pattern to avoid),
    //    tried last, when A has long been up.
    let listener = wildcard.matching_listener().await.unwrap();
    let matched = tokio::time::timeout(Duration::from_secs(2), async {
        while let Ok(status) = listener.recv_async().await {
            if status.matching() {
                return;
            }
        }
    });
    let gated = if wildcard.matching_status().await.unwrap().matching() || matched.await.is_ok() {
        query(&wildcard).await
    } else {
        Vec::new() // no match within 2 s: the gated caller gives up
    };
    println!("3. gated on matching: {} replies {gated:?}", gated.len());

    if gated.is_empty() && retried.len() == 2 && concrete.len() == 2 {
        println!(
            "OK: the gated query got nothing, but retrying the wildcard got {} and concrete keys got {} ({})",
            retried.len(),
            concrete.len(),
            zenoh::GIT_VERSION
        );
    } else if retried.len() == 2 && concrete.len() == 2 {
        println!(
            "OK: all three got the replies; the gate works on this version ({})",
            zenoh::GIT_VERSION
        );
    } else {
        println!(
            "FAILED: gated={} retried={} concrete={} ({})",
            gated.len(),
            retried.len(),
            concrete.len(),
            zenoh::GIT_VERSION
        );
    }
    // Closing or dropping a session blocks for about 10 s, so exit right away.
    std::process::exit(0);
}
