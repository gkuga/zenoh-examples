//! How long a `put` waits while a peer joins a session that holds many
//! declarations, with the two sessions on two hosts.
//!
//! Session A (`serve`) declares N queryables and puts a sample every 10 ms. It
//! keeps a histogram of every put's duration, and reports the longest put wait
//! between session B's connection and B's first reply. Session B (the default
//! command) joins A as a *peer* over TCP and queries one key until it gets a reply.
//!
//! Usage:
//!   peer-join-remote serve N ENDPOINT    session A, listening on ENDPOINT
//!   peer-join-remote N [DEADLINE_S [LINGER_S]]
//!                                        session B, connecting to $PJS_REMOTE;
//!                                        stays LINGER_S after its first reply

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zenoh::query::Queryable;
use zenoh::Wait;

/// The mode session B joins in.
const JOIN_MODE: &str = "peer";
const TARGET: &str = "demo/target";
const TICK: &str = "demo/tick";
const PUT_INTERVAL: Duration = Duration::from_millis(10);
/// A put that takes this long or longer counts as slow.
const SLOW_MS: u64 = 10;
/// The put durations, in ms, that the report counts puts over.
const OVER_MS: [u64; 5] = [10, 50, 100, 500, 1000];
/// One histogram bucket per ms; the last one holds everything from 60 s up.
const BUCKETS: usize = 60_001;
/// The env var that holds the endpoint session B connects to.
const REMOTE_VAR: &str = "PJS_REMOTE";

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

// ---------------------------------------------------------------- session A

/// Session A: declares `n` queryables plus one that answers, then puts forever.
async fn serve(n: usize, endpoint: &str) {
    zenoh::init_log_from_env_or("error");

    let session = zenoh::open(config("peer", Some(endpoint), None))
        .await
        .unwrap();
    let state = Arc::new(State::new());
    let start = Instant::now();
    // Keep the queryables to the end: dropping one undeclares it.
    let _queryables = declare_queryables(&session, n, state.clone());
    println!(
        "READY {n} queryables declared in {:.2?}; rss {} MB",
        start.elapsed(),
        mem_mb("VmRSS")
    );

    watch_puts(state.clone());
    report_slow_puts(state.clone());
    watch_peers(session.clone(), state.clone());
    exit_on_stdin_close(state.clone());

    tokio::select! {
        _ = put_forever(&session, &state) => {}
        _ = exit_signal() => {}
    }
    state.final_report();
    // Exit right away instead of dropping the session.
    std::process::exit(0);
}

/// Declares `n` queryables that do nothing, on `demo/decl/{i}`, plus one on
/// `TARGET` that answers and notes B's first reply.
fn declare_queryables(session: &zenoh::Session, n: usize, state: Arc<State>) -> Vec<Queryable<()>> {
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
        .callback(move |q| {
            q.reply(TARGET, "ok").wait().unwrap();
            state.join.replied(&state);
        })
        .wait();
    queryables.push(target.unwrap());
    queryables
}

/// Puts on `TICK` every `PUT_INTERVAL`, recording how long each put takes.
async fn put_forever(session: &zenoh::Session, state: &State) {
    loop {
        let start = state.clock.start();
        session.put(TICK, "x").wait().unwrap();
        let end = state.clock.stop();
        state.record_put(start, end);
        tokio::time::sleep(PUT_INTERVAL).await;
    }
}

/// Resolves on SIGTERM or SIGINT.
async fn exit_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).unwrap();
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Exits, with the final report, when stdin closes (e.g. the parent goes away).
fn exit_on_stdin_close(state: Arc<State>) {
    std::thread::spawn(move || {
        let mut line = String::new();
        while std::io::stdin().read_line(&mut line).is_ok_and(|r| r > 0) {}
        state.final_report();
        std::process::exit(0);
    });
}

/// Everything session A's threads share.
struct State {
    clock: PutClock,
    hist: Mutex<Histogram>,
    /// Set when a slow put ended since the last report.
    slow_since_report: AtomicBool,
    join: Join,
}

impl State {
    fn new() -> Self {
        State {
            clock: PutClock::new(),
            hist: Mutex::new(Histogram::new()),
            slow_since_report: AtomicBool::new(false),
            join: Join::default(),
        }
    }

    /// Records a put that ran from `start` to `end` (ms on the clock).
    fn record_put(&self, start: u64, end: u64) {
        let ms = end - start;
        self.hist.lock().unwrap().add(ms);
        if ms >= SLOW_MS {
            self.slow_since_report.store(true, Ordering::SeqCst);
        }
        self.join.put_ended(start, end);
    }

    fn print_stats(&self, label: &str) {
        let line = self.hist.lock().unwrap().summary();
        println!("{label}: {line}");
    }

    fn final_report(&self) {
        self.print_stats("put stats (final)");
        self.join.print_summary(&self.clock);
        println!(
            "rss {} MB, peak rss {} MB",
            mem_mb("VmRSS"),
            mem_mb("VmHWM")
        );
    }
}

/// Put durations at 1 ms resolution: bucket `i` holds the puts that took
/// `i` ms up to (but not including) `i + 1` ms.
struct Histogram {
    counts: Vec<u64>,
    total: u64,
    max_ms: u64,
}

impl Histogram {
    fn new() -> Self {
        Histogram {
            counts: vec![0; BUCKETS],
            total: 0,
            max_ms: 0,
        }
    }

    fn add(&mut self, ms: u64) {
        self.counts[(ms as usize).min(BUCKETS - 1)] += 1;
        self.total += 1;
        self.max_ms = self.max_ms.max(ms);
    }

    /// The smallest bucket at or below which `q` of the puts fall, in ms.
    fn quantile(&self, q: f64) -> u64 {
        let rank = ((self.total as f64) * q).ceil().max(1.0) as u64;
        let mut seen = 0;
        for (ms, &c) in self.counts.iter().enumerate() {
            seen += c;
            if seen >= rank {
                return ms as u64;
            }
        }
        self.max_ms
    }

    /// How many puts took `ms` or longer.
    fn at_least(&self, ms: u64) -> u64 {
        self.counts[(ms as usize).min(BUCKETS - 1)..].iter().sum()
    }

    fn summary(&self) -> String {
        if self.total == 0 {
            return "count=0".into();
        }
        let over: Vec<String> = OVER_MS
            .iter()
            .map(|&ms| format!(">={ms}ms={}", self.at_least(ms)))
            .collect();
        format!(
            "count={} p50={}ms p99={}ms p99.9={}ms max={}ms {}",
            self.total,
            self.quantile(0.50),
            self.quantile(0.99),
            self.quantile(0.999),
            self.max_ms,
            over.join(" ")
        )
    }
}

/// When the put in progress started, readable from another thread.
struct PutClock {
    epoch: Instant,
    /// ms since `epoch` + 1 when the put in progress started, 0 when idle.
    started: AtomicU64,
}

impl PutClock {
    fn new() -> Self {
        PutClock {
            epoch: Instant::now(),
            started: AtomicU64::new(0),
        }
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Marks a put as started; returns its start in ms.
    fn start(&self) -> u64 {
        let now = self.now_ms();
        self.started.store(now + 1, Ordering::SeqCst);
        now
    }

    /// Marks the put as done; returns its end in ms.
    fn stop(&self) -> u64 {
        let now = self.now_ms();
        self.started.store(0, Ordering::SeqCst);
        now
    }

    /// The put in progress as (its start + 1, how long it has been running in
    /// ms), or `None` when idle.
    fn running(&self) -> Option<(u64, u64)> {
        match self.started.load(Ordering::SeqCst) {
            0 => None,
            started => Some((started, (self.now_ms() + 1).saturating_sub(started))),
        }
    }
}

/// Reports, once a second, a put that has not returned within 1 s. It runs on
/// its own thread, since a put blocked on the routing tables' lock blocks its
/// thread.
fn watch_puts(state: Arc<State>) {
    std::thread::spawn(move || {
        // The put reported on (its start), and how many seconds of it were reported.
        let (mut watched, mut reported) = (0, 0);
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let Some((started, ms)) = state.clock.running() else {
                continue;
            };
            if started != watched {
                (watched, reported) = (started, 0);
            }
            let secs = ms / 1000;
            if secs > reported {
                reported = secs;
                println!("put on {TICK} blocked for {secs} s so far");
            }
        }
    });
}

/// Prints the put statistics once a second while puts are slow: when a put of
/// `SLOW_MS` or more ended in the last second, or one is running that long.
fn report_slow_puts(state: Arc<State>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let slow_ended = state.slow_since_report.swap(false, Ordering::SeqCst);
        let running = state.clock.running().map_or(0, |(_, ms)| ms);
        if running >= SLOW_MS {
            state.print_stats(&format!("put stats (one put running for {running} ms)"));
        } else if slow_ended {
            state.print_stats("put stats");
        }
    });
}

/// Notes when the first peer (session B) connects, by polling A's peers on its
/// own thread.
fn watch_peers(session: zenoh::Session, state: Arc<State>) {
    std::thread::spawn(move || loop {
        if session.info().peers_zid().wait().next().is_some() {
            state.join.connected(&state.clock);
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    });
}

/// The window from B's connection to B's first reply, and the longest put wait
/// that overlapped it. Times are ms on the put clock + 1, 0 for not yet.
#[derive(Default)]
struct Join {
    connected: AtomicU64,
    replied: AtomicU64,
    /// The longest finished put that overlapped the window, in ms.
    max_wait: AtomicU64,
}

impl Join {
    fn connected(&self, clock: &PutClock) {
        let now = clock.now_ms();
        if self
            .connected
            .compare_exchange(0, now + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            println!("B connected at {:.3} s", now as f64 / 1000.0);
        }
    }

    /// Called on every reply to `TARGET`; acts on the first one after B connected.
    fn replied(&self, state: &State) {
        if self.connected.load(Ordering::SeqCst) == 0 {
            return;
        }
        let now = state.clock.now_ms();
        if self
            .replied
            .compare_exchange(0, now + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            self.print_summary(&state.clock);
        }
    }

    /// Takes a finished put into account if it overlapped the window.
    fn put_ended(&self, start: u64, end: u64) {
        let connected = self.connected.load(Ordering::SeqCst);
        let replied = self.replied.load(Ordering::SeqCst);
        if connected == 0 || end + 1 < connected {
            return;
        }
        if replied != 0 && start + 1 > replied {
            return;
        }
        self.max_wait.fetch_max(end - start, Ordering::SeqCst);
    }

    /// The longest put wait in the window so far, counting a put in progress
    /// that started in it.
    fn max_wait(&self, clock: &PutClock) -> u64 {
        let mut max = self.max_wait.load(Ordering::SeqCst);
        let replied = self.replied.load(Ordering::SeqCst);
        if let Some((started, ms)) = clock.running() {
            if replied == 0 || started <= replied {
                max = max.max(ms);
            }
        }
        max
    }

    fn print_summary(&self, clock: &PutClock) {
        let connected = self.connected.load(Ordering::SeqCst);
        let replied = self.replied.load(Ordering::SeqCst);
        if connected == 0 {
            println!("join: B never connected");
            return;
        }
        let max = self.max_wait(clock);
        if replied == 0 {
            println!("join: no reply to B yet; max put wait since B connected: {max} ms");
        } else {
            let ms = replied - connected;
            println!(
                "join: first reply to B {:.3} s after B connected; max put wait in between: {max} ms",
                ms as f64 / 1000.0
            );
        }
    }
}

/// A field of /proc/self/status (`VmRSS`, `VmHWM`) in MB, or "?" off Linux.
fn mem_mb(field: &str) -> String {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|l| l.strip_prefix(field)?.strip_prefix(':'))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        .map_or("?".into(), |kb| (kb / 1024).to_string())
}

// ---------------------------------------------------------------- session B

/// Session B: joins A in `JOIN_MODE` and queries `TARGET` until it gets a reply.
async fn join(n: usize, deadline: Duration, linger: Duration) {
    zenoh::init_log_from_env_or("error");
    let Ok(endpoint) = std::env::var(REMOTE_VAR) else {
        eprintln!("set {REMOTE_VAR} to session A's endpoint, e.g. tcp/<A's address>:17501");
        std::process::exit(2);
    };
    println!("[B] joining as {JOIN_MODE} via {endpoint}");
    let start = Instant::now();
    // The session is kept until the exit below, since closing or dropping it
    // blocks for about 10 s.
    let session = zenoh::open(config(JOIN_MODE, None, Some(&endpoint)))
        .await
        .unwrap();
    println!("[B] session open after {:.2?}", start.elapsed());

    match query_until_first_reply(&session, start, deadline).await {
        Some(t) => println!("[B] first reply after {t:.2?} with N={n}"),
        None => println!("[B] no reply within {deadline:?} with N={n}"),
    }
    println!("[B] rss {} MB at first reply", mem_mb("VmRSS"));
    // Stay connected, so that A finishes pushing its declarations to B while
    // A's put wait is measured.
    tokio::time::sleep(linger).await;
    println!(
        "[B] rss {} MB, peak rss {} MB after staying {linger:?}",
        mem_mb("VmRSS"),
        mem_mb("VmHWM")
    );
    std::process::exit(0);
}

/// Queries `TARGET` until a reply arrives or `deadline` has passed since `start`.
/// Returns the time from `start` to the first reply.
async fn query_until_first_reply(
    session: &zenoh::Session,
    start: Instant,
    deadline: Duration,
) -> Option<Duration> {
    let mut noted = 0;
    while start.elapsed() < deadline {
        let get = session
            .get(TARGET)
            .timeout(Duration::from_secs(2))
            .await
            .unwrap();
        while let Ok(reply) = get.recv_async().await {
            if reply.result().is_ok() {
                return Some(start.elapsed());
            }
        }
        // A get with no route ends at once; retry, noting it at most once every 5 s.
        if start.elapsed().as_secs() >= noted + 5 {
            noted = start.elapsed().as_secs();
            println!("[B] no reply yet at {:.2?}", start.elapsed());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("serve") {
        let (Some(n), Some(endpoint)) = (args.get(2), args.get(3)) else {
            eprintln!("usage: peer-join-remote serve N ENDPOINT");
            std::process::exit(2);
        };
        return serve(n.parse().unwrap(), endpoint).await;
    }
    let n: usize = args.get(1).map_or(200_000, |s| s.parse().unwrap());
    let deadline = args.get(2).map_or(30, |s| s.parse().unwrap());
    let linger = args.get(3).map_or(10, |s| s.parse().unwrap());
    join(
        n,
        Duration::from_secs(deadline),
        Duration::from_secs(linger),
    )
    .await;
}
