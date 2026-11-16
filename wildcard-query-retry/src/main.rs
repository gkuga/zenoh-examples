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

use zenoh::query::{Querier, Queryable};

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

/// Opens peer A listening on `endpoint` and peer B connecting to it, and waits
/// until B sees A.
async fn open_linked_pair(endpoint: &str) -> (zenoh::Session, zenoh::Session) {
    let a = zenoh::open(config("listen", endpoint)).await.unwrap();
    let b = zenoh::open(config("connect", endpoint)).await.unwrap();
    while b.info().peers_zid().await.count() == 0 {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    (a, b)
}

/// Declares a queryable on each of `QUERYABLES`, replying from its own key.
async fn declare_queryables(session: &zenoh::Session) -> Vec<Queryable<()>> {
    let mut queryables = Vec::new();
    for key in QUERYABLES {
        let q = session.declare_queryable(key).callback(move |q| {
            tokio::spawn(async move { q.reply(key, "v").await.unwrap() });
        });
        queryables.push(q.await.unwrap());
    }
    queryables
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

    // The sessions are kept until the exit below, since closing or dropping one
    // blocks for about 10 s.
    let (a, b) = open_linked_pair(&endpoint).await;
    start_service_late(a);
    let start = Instant::now();

    let wildcard = b
        .declare_querier(WILDCARD)
        .timeout(Duration::from_millis(500))
        .await
        .unwrap();
    let retried = query_with_retry(&wildcard, start).await;
    let concrete = query_concrete(&b).await;
    let gated = query_gated(&wildcard).await;
    report(&retried, &concrete, &gated);
    // Exit right away instead of dropping the sessions.
    std::process::exit(0);
}

/// A's service comes up 300 ms from now, a little later than B.
fn start_service_late(a: zenoh::Session) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Keep the queryables (and A) to the end: dropping one undeclares it.
        let _queryables = declare_queryables(&a).await;
        std::future::pending::<()>().await;
    });
}

/// 1. Queries the wildcard with a timeout and retries, up to 5 times; the replies
///    themselves say who is there.
async fn query_with_retry(wildcard: &Querier<'_>, start: Instant) -> Vec<String> {
    for attempt in 1..=5 {
        let keys = query(wildcard).await;
        println!(
            "1. retry attempt {attempt} at {:.2?}: {} replies",
            start.elapsed(),
            keys.len()
        );
        if !keys.is_empty() {
            return keys;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Vec::new()
}

/// 2. Queries through a querier per concrete key, whose matching status is reliable.
///    Returns the keys that answered a querier reporting a match.
async fn query_concrete(session: &zenoh::Session) -> Vec<String> {
    let mut concrete = Vec::new();
    for key in QUERYABLES {
        let querier = session.declare_querier(key).await.unwrap();
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
    concrete
}

/// 3. Queries the wildcard only once its querier reports a match, waiting up to
///    2 s for it (the pattern to avoid). Tried last, when A has long been up.
async fn query_gated(wildcard: &Querier<'_>) -> Vec<String> {
    let listener = wildcard.matching_listener().await.unwrap();
    let matched = tokio::time::timeout(Duration::from_secs(2), async {
        while let Ok(status) = listener.recv_async().await {
            if status.matching() {
                return;
            }
        }
    });
    let gated = if wildcard.matching_status().await.unwrap().matching() || matched.await.is_ok() {
        query(wildcard).await
    } else {
        Vec::new() // no match within 2 s: the gated caller gives up
    };
    println!("3. gated on matching: {} replies {gated:?}", gated.len());
    gated
}

/// Prints the verdict: retrying and concrete keys should get both replies, and on
/// 1.9.0 and later the gated query nothing.
fn report(retried: &[String], concrete: &[String], gated: &[String]) {
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
}
