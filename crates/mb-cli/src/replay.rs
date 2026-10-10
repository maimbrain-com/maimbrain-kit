//! `mb replay`: runs a replay log on a bundle with no screen and reports what
//! the run scored and its draw hash (docs/UPDATES.md "The headless runner").
//!
//! The run happens in Deno, in the same TypeScript runtime and mb-host the
//! phone uses (runtime/src/headless.ts), with mb-host initialized without a
//! GPU: everything a game reads back is computed on the CPU, so the run goes
//! as it did on the phone. The bundle is staged as the app serves it (its
//! wasm loop-metered), and Deno may read only that staging directory.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

macro_rules! embedded {
    ($f:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/runtime/", $f)) as &[u8]
    };
}

/// The runtime files the headless runner needs.
const FILES: [(&str, &[u8]); 2] = [("headless.js", embedded!("headless.js")), ("mb_host_bg.wasm", embedded!("mb_host_bg.wasm"))];

pub struct Options {
    pub json: bool,
    /// A built runtime (runtime/dist) to use instead of the one in this mb.
    pub runtime: Option<PathBuf>,
    pub timeout: Duration,
}

pub fn replay(bundle: &Path, log: &Path, opts: &Options) -> Result<(), String> {
    let bytes = std::fs::read(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
    let b = mb_format::Bundle::open(&bytes).map_err(|r| format!("{} is invalid: {}", bundle.display(), r.errors.join("; ")))?;
    let log = read_log(log)?;
    let recorded_on = log["bundle"].as_str().map(str::to_string);

    let work = std::env::temp_dir().join(format!("mb-replay-{}-{}", std::process::id(), nanos()));
    let result = run(bundle, &log, &work, opts);
    let _ = std::fs::remove_dir_all(&work);
    let mut r = result?;

    // What it ran on, and whether that's the build the run was recorded on.
    r["bundle"] = b.sha256.clone().into();
    r["recorded_on"] = recorded_on.clone().map_or(Value::Null, Value::from);
    if opts.json {
        println!("{r}");
    } else {
        print_summary(&r, &b.manifest.id, recorded_on.as_deref(), &b.sha256);
    }
    Ok(())
}

/// A replay log, or a file that wraps one under `log` (the app's saved best
/// run, a test's recording).
fn read_log(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
    let log = if v["frames"].is_array() { v } else { v["log"].clone() };
    if !log["frames"].is_array() || !log["seed"].is_string() {
        return Err(format!("{}: not a replay log (no frames or seed)", path.display()));
    }
    Ok(log)
}

fn run(bundle: &Path, log: &Value, work: &Path, opts: &Options) -> Result<Value, String> {
    let game = work.join("game");
    let manifest = crate::unpack(bundle, &game)?;
    let boot = crate::boot_json(&manifest, 0, "2026-01-01");
    std::fs::write(game.join("boot.json"), boot.to_string()).map_err(|e| e.to_string())?;
    std::fs::write(work.join("log.json"), log.to_string()).map_err(|e| e.to_string())?;
    let runtime = work.join("runtime");
    std::fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    for (name, embedded) in FILES {
        let path = runtime.join(name);
        match &opts.runtime {
            Some(dir) => std::fs::copy(dir.join(name), &path).map(|_| ()).map_err(|e| format!("{}: {e}", dir.join(name).display()))?,
            None if embedded.is_empty() => return Err(missing_runtime()),
            None => std::fs::write(&path, embedded).map_err(|e| e.to_string())?,
        }
    }

    let deno = std::env::var_os("MB_DENO").map_or_else(|| PathBuf::from("deno"), PathBuf::from);
    let mut child = Command::new(&deno)
        .args(["run", "--quiet", "--no-prompt", "--no-config", "--no-npm", "--no-remote"])
        .arg(format!("--allow-read={}", work.display()))
        .arg(runtime.join("headless.js"))
        .arg(&game)
        .arg(work.join("log.json"))
        // Deno's own cache goes with the rest, not in the user's home.
        .env("DENO_DIR", work.join("deno"))
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                format!("mb replay needs Deno ({} wasn't found): install it from https://deno.com, or set MB_DENO to it", deno.display())
            } else {
                format!("{}: {e}", deno.display())
            }
        })?;
    // Each pipe on its own thread: a game that logs a lot fills stderr while
    // stdout is still open, and Deno would block on the write.
    let drain = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = pipe.read_to_string(&mut s);
            s
        })
    };
    let out = drain(Box::new(child.stdout.take().unwrap()));
    let err = drain(Box::new(child.stderr.take().unwrap()));
    let deadline = Instant::now() + opts.timeout;
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("the replay took longer than {} s; stopped it", opts.timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let (stdout, stderr) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    let line = stdout.lines().rev().find(|l| l.starts_with('{'));
    match (status.success(), line) {
        (true, Some(l)) => serde_json::from_str(l).map_err(|e| format!("the runner's output isn't JSON ({e}): {l}")),
        // The error itself, not the last line of its stack trace.
        _ => Err(format!(
            "the runner failed ({status}): {}",
            stderr.lines().find(|l| l.contains("Error")).or_else(|| stderr.trim().lines().last()).unwrap_or("no output")
        )),
    }
}

fn print_summary(r: &Value, game: &str, recorded_on: Option<&str>, sha: &str) {
    let n = |k: &str| r[k].as_u64().unwrap_or(0);
    println!("{game}: ran {} of {} frames in {} ms · draw hash {}", n("frames"), n("of"), n("ms"), r["hash"].as_str().unwrap_or("?"));
    match recorded_on {
        // `s` comes from the log: cut it by characters, not bytes.
        Some(s) if s != sha => println!("  recorded on another build ({}…); this is {}…", short(s), short(sha)),
        Some(_) => println!("  recorded on this build"),
        None => println!("  the log doesn't say which build it was recorded on"),
    }
    for s in r["scores"].as_array().into_iter().flatten() {
        println!("  score: board {} = {} (frame {})", s["board"], s["value"].as_str().unwrap_or("?"), s["frame"]);
    }
    if r["scores"].as_array().is_none_or(|a| a.is_empty()) {
        println!("  no scores submitted");
    }
    if let Some(e) = r["error"].as_str() {
        println!("  stopped early: {e}");
    }
    match n("missingAssets") {
        0 => {}
        1 => println!("  an asset the run saw arrive isn't in this build: it went differently here"),
        k => println!("  {k} assets the run saw arrive aren't in this build: it went differently here"),
    }
}

fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

fn missing_runtime() -> String {
    format!(
        "this mb was built without the web runtime ({} was missing). In the Maimbrain repo, run `just runtime`, then run mb \
         again through cargo; or pass --runtime <dir> with a built runtime/dist",
        env!("MB_RUNTIME_DIST")
    )
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}
