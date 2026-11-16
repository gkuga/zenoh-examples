//! A peer joining a session that holds many declarations stalls (zenoh 1.8.0).
//!
//! Session A (a child process) declares N queryables and keeps putting a sample
//! every 100 ms. Session B joins it as a *peer* over loopback TCP and queries one key.
//! In peer mode A pushes every declaration to B while holding its routing tables'
//! write lock, so B waits for its first reply and A's own puts freeze.
//!
//! Usage: peer-join-stall [N] [PORT]

use std::io::{BufRead, BufReader, Lines};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use zenoh::Wait;

/// The mode session B joins in (`client-join` sets "client" here).
const JOIN_MODE: &str = "peer";
const TARGET: &str = "demo/target";
const DEADLINE: Duration = Duration::from_secs(30);

fn config(mode: &str, listen: Option<&str>, connect: Option<&str>) -> zenoh::Config {
    let mut c = zenoh::Config::default();
    c.insert_json5("mode", &format!("\"{mode}\"")).unwrap();
    // No multicast scouting: the sessions only meet through the endpoints below.
    c.insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    let listen = listen.map(|e| format!("[\"{e}\"]")).unwrap_or("[]".into());
    c.insert_json5("listen/endpoints", &listen).unwrap();
    if let Some(e) = connect {
        c.insert_json5("connect/endpoints", &format!("[\"{e}\"]"))
            .unwrap();
    }
    c
}

/// Session A: declares `n` queryables plus one that answers, then puts forever.
async fn serve(n: usize, endpoint: &str) {
    zenoh::init_log_from_env_or("error");
    // Exit when the parent goes away (its end of our stdin closes).
    std::thread::spawn(|| {
        let mut line = String::new();
        while std::io::stdin()
            .read_line(&mut line)
            .map_or(false, |r| r > 0)
        {}
        std::process::exit(0);
    });

    let session = zenoh::open(config("peer", Some(endpoint), None))
        .await
        .unwrap();
    let start = Instant::now();
    let mut queryables = Vec::with_capacity(n + 1);
    for i in 0..n {
        let q = session
            .declare_queryable(format!("demo/decl/{i}"))
            .callback(|_| {})
            .wait();
        queryables.push(q.unwrap());
    }
    let target = session.declare_queryable(TARGET);
    let target = target
        .callback(|q| q.reply(TARGET, "ok").wait().unwrap())
        .wait();
    queryables.push(target.unwrap());
    println!("READY {n} queryables declared in {:.2?}", start.elapsed());

    // A watchdog thread reports a put that does not return within 1 s,
    // since a put blocked on the routing tables' lock blocks its thread.
    let epoch = Instant::now();
    let put_started = Arc::new(AtomicU64::new(0)); // ms since epoch + 1, 0 = idle
    let watched = put_started.clone();
    std::thread::spawn(move || {
        let mut reported = 0;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let started = watched.load(Ordering::SeqCst);
            if started == 0 {
                reported = 0;
                continue;
            }
            let stuck = epoch.elapsed().as_millis() as u64 + 1 - started;
            if stuck / 1000 > reported {
                reported = stuck / 1000;
                println!("put on demo/tick blocked for {reported} s so far");
            }
        }
    });
    loop {
        put_started.store(epoch.elapsed().as_millis() as u64 + 1, Ordering::SeqCst);
        session.put("demo/tick", "x").wait().unwrap();
        put_started.store(0, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Session A's stdout, line by line.
type ChildLines = Lines<BufReader<ChildStdout>>;

/// What the parent process reads off session A's output, while session B runs.
#[derive(Default)]
struct Effects {
    /// The longest time one of A's puts has been blocked, in s.
    put_blocked: AtomicUsize,
    /// How many times A has closed the link.
    closes: AtomicUsize,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("serve") {
        return serve(args[2].parse().unwrap(), &args[3]).await;
    }
    let n: usize = args.get(1).map_or(40_000, |s| s.parse().unwrap());
    let port: u16 = args.get(2).map_or(17501, |s| s.parse().unwrap());
    let endpoint = format!("tcp/127.0.0.1:{port}");

    // Job 1: watch session A. Wait until it has declared everything, then keep
    // counting what it reports on another thread.
    let (mut child, mut lines) = spawn_session_a(n, &endpoint);
    wait_until_ready(&mut lines);
    let effects = count_effects(lines);

    // Job 2: session B joins A and queries it. The session is kept until the
    // exit below, since closing or dropping it blocks for about 10 s.
    let (session, start) = join(&endpoint).await;
    let first_reply = query_until_first_reply(&session, start).await;

    // Let A's watchdog catch up before reading its counters.
    tokio::time::sleep(Duration::from_millis(500)).await;
    report(n, first_reply, &effects);

    let _ = child.kill();
    let _ = child.wait();
    // Exit right away instead of dropping the session.
    std::process::exit(0);
}

/// Starts session A in a child process (`serve`), so the two sessions do not
/// share zenoh's runtime threads. A exits when its stdin closes, i.e. when this
/// process goes away.
fn spawn_session_a(n: usize, endpoint: &str) -> (Child, ChildLines) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["serve", &n.to_string(), endpoint])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap()).lines();
    (child, lines)
}

/// Echoes A's output until A reports that all its queryables are declared.
fn wait_until_ready(lines: &mut ChildLines) {
    loop {
        let line = lines.next().expect("session A exited").unwrap();
        println!("[A] {line}");
        if line.starts_with("READY") {
            return;
        }
    }
}

/// Keeps echoing A's output on its own thread, so that reading it does not
/// hold up session B, and counts A's blocked puts and closed links.
fn count_effects(lines: ChildLines) -> Arc<Effects> {
    let effects = Arc::new(Effects::default());
    let counted = effects.clone();
    std::thread::spawn(move || {
        for line in lines.map_while(Result::ok) {
            // "put on demo/tick blocked for {s} s so far": keep the longest.
            if let Some(s) = line.strip_prefix("put on demo/tick blocked for ") {
                let secs = s.split(' ').next().unwrap().parse().unwrap_or(0);
                counted.put_blocked.fetch_max(secs, Ordering::SeqCst);
            }
            if line.contains("Closing transport") {
                counted.closes.fetch_add(1, Ordering::SeqCst);
            }
            println!("[A] {line}");
        }
    });
    effects
}

/// Session B joins A in `JOIN_MODE`. Returns the session and when the join started.
async fn join(endpoint: &str) -> (zenoh::Session, Instant) {
    println!("[B] joining as {JOIN_MODE} via {endpoint}");
    let start = Instant::now();
    let session = zenoh::open(config(JOIN_MODE, None, Some(endpoint)))
        .await
        .unwrap();
    println!("[B] session open after {:.2?}", start.elapsed());
    (session, start)
}

/// Queries `TARGET` until a reply arrives or `DEADLINE` has passed since `start`.
/// Returns the time from `start` to the first reply.
async fn query_until_first_reply(session: &zenoh::Session, start: Instant) -> Option<Duration> {
    let mut noted = 0;
    while start.elapsed() < DEADLINE {
        let get = session
            .get(TARGET)
            .timeout(Duration::from_secs(2))
            .await
            .unwrap();
        let mut first_reply = None;
        while let Ok(reply) = get.recv_async().await {
            if reply.result().is_ok() {
                first_reply = Some(start.elapsed());
            }
        }
        if first_reply.is_some() {
            return first_reply;
        }
        // A get with no route ends at once; retry, noting it at most once a second.
        if start.elapsed().as_secs() > noted {
            noted = start.elapsed().as_secs();
            println!("[B] no reply yet at {:.2?}", start.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

/// Prints the verdict line.
fn report(n: usize, first_reply: Option<Duration>, effects: &Effects) {
    let blocked = effects.put_blocked.load(Ordering::SeqCst);
    let closes = effects.closes.load(Ordering::SeqCst);
    let summary =
        format!("A's put blocked for up to {blocked} s; A closed the link {closes} time(s)");
    match first_reply {
        Some(t) if t < Duration::from_secs(2) && blocked == 0 => {
            println!("OK: {JOIN_MODE} B got its first reply after {t:.2?} with N={n}; {summary}")
        }
        Some(t) => println!("REPRODUCED: {JOIN_MODE} B got its first reply only after {t:.2?} with N={n}; {summary}"),
        None => println!("REPRODUCED: {JOIN_MODE} B got no reply within {DEADLINE:?} with N={n}; {summary}"),
    }
}
