//! `mb replay`: runs a replay log on a bundle with no screen and reports what
//! the run scored and its draw hash (docs/UPDATES.md "The headless runner").
//!
//! The run happens in Deno, in the same TypeScript runtime and mb-host the
//! phone uses (runtime/src/headless.ts), with mb-host initialized without a
//! GPU: everything a game reads back is computed on the CPU, so the run goes
//! as it did on the phone. The bundle is staged as the app serves it (its
//! wasm loop-metered), and Deno may read only that staging directory.
//!
//! `mb play` can also have it draw some frames (`Capture`): those go to a
//! WebGPU device with no canvas (a GPU, or Mesa's software lavapipe on a
//! server), and Deno may write only where they go.

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
/// The runtime files the difficulty bot's worker needs (`mb difficulty`).
pub(crate) const BOT_FILES: [(&str, &[u8]); 2] = [("botrun.js", embedded!("botrun.js")), ("mb_host_bg.wasm", embedded!("mb_host_bg.wasm"))];

pub struct Options {
    pub json: bool,
    /// A built runtime (runtime/dist) to use instead of the one in this mb.
    pub runtime: Option<PathBuf>,
    pub timeout: Duration,
}

/// Frames to draw on a WebGPU device and keep, as raw RGBA: the result's
/// `captured` lists them ({ frame, file }), or `captureError` says why not.
pub(crate) struct Capture {
    /// Frames counted from 1 (the first frame the log runs).
    pub frames: Vec<usize>,
    /// Device pixels.
    pub size: [u32; 2],
    /// Also each frame's variants for `mb play --check readability`
    /// (runtime/src/headless.ts): `<frame>.clean.rgba`, `.subject.rgba`,
    /// and for a 3D subject `.plain.rgba` and `.plain-without.rgba`.
    pub readability: bool,
}

/// A variant of captured frame `f` (`Capture::readability`) as an image,
/// or None when the runner didn't draw it.
pub(crate) fn captured_variant(work: &Path, c: &Capture, f: usize, variant: &str) -> Option<crate::generate::img::Image> {
    let px = std::fs::read(work.join("capture").join(format!("{f}.{variant}.rgba"))).ok()?;
    let [w, h] = c.size;
    (px.len() == (w * h * 4) as usize).then(|| crate::generate::img::Image { w, h, px: px.chunks_exact(4).map(|p| [p[0], p[1], p[2], 255]).collect() })
}

/// `frames`: ones to draw (`Capture`), saved in `out` as frame-<n>.png.
pub fn replay(bundle: &Path, log: &Path, opts: &Options, frames: &[usize], out: &Path) -> Result<(), String> {
    // Opened once (its engine's metered wasm kept, `engines`), and run as opened.
    let b = crate::engines::open(bundle)?;
    let log = read_log(log)?;
    let recorded_on = log["bundle"].as_str().map(str::to_string);

    let work = std::env::temp_dir().join(format!("mb-replay-{}-{}", std::process::id(), nanos()));
    // Twice the screen the run began on (the log's, which a turned game has
    // turned), as `mb play` and the 2x phones draw it. A later Resize is
    // letterboxed into it.
    let screen = |i: usize| log["screen"][i].as_f64().filter(|v| *v >= 1.0).map(|v| v.round() as u32);
    let [lw, lh] = match (screen(0), screen(1)) {
        (Some(w), Some(h)) => [w, h],
        _ => b.manifest.logical_size,
    };
    let capture = (!frames.is_empty()).then(|| Capture { frames: frames.to_vec(), size: [lw * 2, lh * 2], readability: false });
    let result = run(&b, &log, &work, opts, capture.as_ref()).and_then(|(mut r, _)| {
        if let Some(c) = &capture {
            r["shots"] = save_captures(&work, c, out, |f| format!("frame-{f}.png"))?.into_iter().map(|(f, name)| serde_json::json!({ "frame": f, "file": name })).collect();
        }
        Ok(r)
    });
    let _ = std::fs::remove_dir_all(&work);
    let mut r = result?;
    if let Some(o) = r.as_object_mut() {
        o.remove("captured");
    }

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

/// Runs `log` on an opened bundle in the headless runner: its result, and
/// what the run wrote to stderr (the game's log lines, `[game] …`). Captured
/// frames are in `work`/capture.
pub(crate) fn run(b: &mb_format::Bundle, log: &Value, work: &Path, opts: &Options, capture: Option<&Capture>) -> Result<(Value, String), String> {
    let game = work.join("game");
    crate::unpack(b, &game)?;
    let boot = crate::boot_json(&b.manifest, 0, "2026-01-01");
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
    let mut cmd = Command::new(&deno);
    cmd.args(["run", "--quiet", "--no-prompt", "--no-config", "--no-npm", "--no-remote"]).arg(format!("--allow-read={}", work.display()));
    if let Some(c) = capture {
        let out = work.join("capture");
        std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        let job = serde_json::json!({ "frames": c.frames, "out": out, "width": c.size[0], "height": c.size[1], "readability": c.readability });
        std::fs::write(work.join("capture.json"), job.to_string()).map_err(|e| e.to_string())?;
        cmd.arg(format!("--allow-write={}", out.display()));
    }
    cmd.arg(runtime.join("headless.js")).arg(&game).arg(work.join("log.json"));
    if capture.is_some() {
        cmd.arg(work.join("capture.json"));
    }
    let mut child = cmd
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
        (true, Some(l)) => serde_json::from_str(l).map(|v| (v, stderr.clone())).map_err(|e| format!("the runner's output isn't JSON ({e}): {l}")),
        // The error itself, not the last line of its stack trace.
        _ => Err(format!(
            "the runner failed ({status}): {}",
            stderr.lines().find(|l| l.contains("Error")).or_else(|| stderr.trim().lines().last()).unwrap_or("no output")
        )),
    }
}

/// The frames a `run` captured (in `work`), saved in `out` as PNGs named
/// `name(frame)`: each frame and its file name, in frame order.
pub(crate) fn save_captures(work: &Path, c: &Capture, out: &Path, name: impl Fn(usize) -> String) -> Result<Vec<(usize, String)>, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut saved = Vec::new();
    for &f in &c.frames {
        // A frame past the run's end, or none at all when there was no device.
        let Ok(px) = std::fs::read(work.join("capture").join(format!("{f}.rgba"))) else { continue };
        let [w, h] = c.size;
        if px.len() != (w * h * 4) as usize {
            return Err(format!("captured frame {f} is {} bytes, not {w}×{h} RGBA", px.len()));
        }
        let file = name(f);
        std::fs::write(out.join(&file), frame_png(&px, c.size)?).map_err(|e| format!("{}: {e}", out.join(&file).display()))?;
        saved.push((f, file));
    }
    saved.sort();
    saved.dedup();
    Ok(saved)
}

/// An opaque frame as a PNG, encoded once and fast: `Image::encode_png`
/// tries every filter at the strongest compression, a second a frame here.
fn frame_png(rgba: &[u8], [w, h]: [u32; 2]) -> Result<Vec<u8>, String> {
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
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
    for s in r["shots"].as_array().into_iter().flatten() {
        println!("  frame {}: {}", s["frame"], s["file"].as_str().unwrap_or("?"));
    }
    if let Some(e) = r["captureError"].as_str() {
        println!("  frames not drawn: {e}");
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

pub(crate) fn missing_runtime() -> String {
    format!(
        "this mb was built without the web runtime ({} was missing). In the Maimbrain repo, run `just runtime`, then run mb \
         again through cargo; or pass --runtime <dir> with a built runtime/dist",
        env!("MB_RUNTIME_DIST")
    )
}

fn nanos() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
}
