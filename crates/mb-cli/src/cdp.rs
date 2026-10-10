//! A headless Chrome (or Chromium, or Edge) driven over the Chrome DevTools
//! Protocol, for `mb shot`.
//!
//! Chrome is started with its own throwaway profile and
//! `--remote-debugging-port=0`; it writes the port it picked to
//! DevToolsActivePort in the profile, and we talk to it over a WebSocket.
//! The client is the few lines of RFC 6455 a localhost connection needs
//! (masked text frames out, unmasked frames in; live.rs has the server
//! side), so no WebSocket crate is pulled in. A reader thread turns frames
//! into JSON messages; command results are matched by id, and everything
//! else (console output, exceptions, crashes) is kept as events.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long a single command may take (stepping a few seconds of a heavy 3D game included).
const CALL_TIMEOUT: Duration = Duration::from_secs(120);

/// The browser to use: `explicit` (`--chrome`), else `MB_CHROME`, else the
/// usual install locations and names on PATH for this OS.
pub fn find_chrome(explicit: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return if p.is_file() { Ok(p.to_path_buf()) } else { Err(format!("--chrome {}: no such file", p.display())) };
    }
    if let Some(p) = std::env::var_os("MB_CHROME").filter(|p| !p.is_empty()) {
        let p = PathBuf::from(p);
        return if p.is_file() { Ok(p) } else { Err(format!("MB_CHROME={}: no such file", p.display())) };
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(target_os = "macos") {
        let apps = [
            "Google Chrome.app/Contents/MacOS/Google Chrome",
            "Chromium.app/Contents/MacOS/Chromium",
            "Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
            "Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];
        let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
        for base in [Some(PathBuf::from("/Applications")), home].into_iter().flatten() {
            candidates.extend(apps.iter().map(|a| base.join(a)));
        }
    } else if cfg!(windows) {
        let rel = [r"Google\Chrome\Application\chrome.exe", r"Chromium\Application\chrome.exe", r"Microsoft\Edge\Application\msedge.exe"];
        for var in ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"] {
            if let Some(base) = std::env::var_os(var) {
                candidates.extend(rel.iter().map(|r| PathBuf::from(&base).join(r)));
            }
        }
    }
    let names = ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome", "microsoft-edge"];
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for n in names {
            candidates.push(dir.join(if cfg!(windows) { format!("{n}.exe") } else { n.to_string() }));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| {
        "no Chrome or Chromium found (mb shot renders the game in headless Chrome with WebGPU). Install Google Chrome, \
         or point --chrome (or MB_CHROME) at a Chromium-based browser"
            .to_string()
    })
}

/// A running headless browser and our connection to it. Dropping it closes the browser.
pub struct Chrome {
    child: Child,
    profile: PathBuf,
    out: TcpStream,
    incoming: Receiver<Result<Value, String>>,
    next_id: u64,
    /// Events (messages with a `method`) not yet looked at.
    events: VecDeque<Value>,
}

impl Chrome {
    /// Starts `exe` headless with a `w`×`h` window and connects to it.
    pub fn launch(exe: &Path, w: u32, h: u32) -> Result<Chrome, String> {
        let profile = std::env::temp_dir().join(format!("mb-shot-chrome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).map_err(|e| format!("{}: {e}", profile.display()))?;
        let log = std::fs::File::create(profile.join("chrome.log")).map_err(|e| e.to_string())?;
        // Chrome's Energy Saver caps every page at 30 fps on battery (by default below 20%),
        // which `mb measure` would read as a slow game: off in this throwaway profile.
        let _ = std::fs::write(profile.join("Local State"), r#"{"performance_tuning":{"battery_saver_mode":{"state":0}}}"#);
        let mut cmd = Command::new(exe);
        cmd.arg("--headless")
            .arg("--remote-debugging-port=0")
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(format!("--window-size={w},{h}"))
            .args(["--no-first-run", "--no-default-browser-check", "--disable-extensions", "--disable-sync", "--disable-background-networking"])
            .args(["--mute-audio", "--hide-scrollbars", "--autoplay-policy=no-user-gesture-required"])
            // A page nobody looks at still gets its timers and frames.
            .args(["--disable-background-timer-throttling", "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows"])
            .arg("--enable-unsafe-webgpu");
        if cfg!(target_os = "linux") {
            cmd.arg("--enable-features=Vulkan");
        }
        cmd.arg("about:blank").stdin(Stdio::null()).stdout(Stdio::null()).stderr(log);
        let mut child = cmd.spawn().map_err(|e| format!("starting {}: {e}", exe.display()))?;
        match devtools_port(&mut child, &profile).and_then(|(port, path)| connect(port, &path)) {
            Ok((out, incoming)) => Ok(Chrome { child, profile, out, incoming, next_id: 0, events: VecDeque::new() }),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_dir_all(&profile);
                Err(e)
            }
        }
    }

    /// Sends a command (to a page when `session` is given) and returns its result.
    pub fn call(&mut self, session: Option<&str>, method: &str, params: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = s.into();
        }
        self.out.write_all(&client_frame(msg.to_string().as_bytes(), mask_key(id))).map_err(|e| format!("{method}: {e}"))?;
        let deadline = Instant::now() + CALL_TIMEOUT;
        loop {
            let mut v = self.recv(deadline)?.ok_or_else(|| format!("{method}: no answer from the browser in {} s", CALL_TIMEOUT.as_secs()))?;
            if v["id"].as_u64() == Some(id) {
                if let Some(e) = v.get("error") {
                    return Err(format!("{method}: {}", e["message"].as_str().unwrap_or("failed")));
                }
                return Ok(v["result"].take());
            }
            if v.get("method").is_some() {
                self.events.push_back(v);
            }
        }
    }

    /// The next event, waiting until `deadline`; None if none came.
    pub fn event(&mut self, deadline: Instant) -> Result<Option<Value>, String> {
        if let Some(e) = self.events.pop_front() {
            return Ok(Some(e));
        }
        while let Some(v) = self.recv(deadline)? {
            if v.get("method").is_some() {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Events that have arrived so far, without waiting.
    pub fn drain_events(&mut self) -> Result<Vec<Value>, String> {
        while let Some(v) = self.recv(Instant::now())? {
            if v.get("method").is_some() {
                self.events.push_back(v);
            }
        }
        Ok(self.events.drain(..).collect())
    }

    fn recv(&mut self, deadline: Instant) -> Result<Option<Value>, String> {
        match self.incoming.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(r) => r.map(Some),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(format!("lost the connection to the browser: {}", log_tail(&self.profile))),
        }
    }
}

/// Waits for the browser to write DevToolsActivePort in its profile: its
/// port, then the browser's WebSocket path.
fn devtools_port(child: &mut Child, profile: &Path) -> Result<(u16, String), String> {
    let file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(text) = std::fs::read_to_string(&file) {
            let mut lines = text.lines();
            if let (Some(Ok(port)), Some(path)) = (lines.next().map(str::parse::<u16>), lines.next()) {
                return Ok((port, path.to_string()));
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("the browser exited ({status}) before it was ready: {}", log_tail(profile)));
        }
        if Instant::now() > deadline {
            return Err(format!("the browser didn't start within 30 s: {}", log_tail(profile)));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The end of the browser's own log, for error messages.
fn log_tail(profile: &Path) -> String {
    let text = std::fs::read_to_string(profile.join("chrome.log")).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(5)..].join(" | ");
    if tail.is_empty() { "(no output)".into() } else { tail }
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

/// Opens the WebSocket at ws://127.0.0.1:`port``path`. Returns the stream to
/// write to and a channel of the messages read from it.
fn connect(port: u16, path: &str) -> Result<(TcpStream, Receiver<Result<Value, String>>), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connecting to the browser: {e}"))?;
    let _ = stream.set_nodelay(true);
    let key = "bWItc2hvdC1jbGllbnQta2V5"; // any 16 bytes, base64
    write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut status = String::new();
    reader.read_line(&mut status).map_err(|e| e.to_string())?;
    if !status.starts_with("HTTP/1.1 101") {
        return Err(format!("the browser refused the DevTools connection: {}", status.trim()));
    }
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 || line == "\r\n" {
            break;
        }
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        loop {
            let msg = read_message(&mut reader).and_then(|text| serde_json::from_slice::<Value>(&text).map_err(|e| format!("bad message from the browser: {e}")));
            let stop = msg.is_err();
            if tx.send(msg).is_err() || stop {
                break;
            }
        }
    });
    Ok((stream, rx))
}

/// One masked, unfragmented client text frame (RFC 6455 §5.2: clients must mask).
fn client_frame(payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut f = vec![0x81];
    match payload.len() {
        n if n < 126 => f.push(0x80 | n as u8),
        n if n <= 0xffff => {
            f.push(0x80 | 126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(0x80 | 127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(&mask);
    f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    f
}

/// A mask that differs per frame (it only has to be unpredictable to
/// intermediaries, and there are none on localhost).
fn mask_key(n: u64) -> [u8; 4] {
    ((n as u32).wrapping_mul(0x9e37_79b9) ^ 0x5bd1_e995).to_be_bytes()
}

/// Reads frames until a whole text or binary message has arrived (joining
/// continuation frames). Pings and pongs are skipped; a close frame ends it.
fn read_message(r: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut message = Vec::new();
    loop {
        let mut head = [0u8; 2];
        r.read_exact(&mut head).map_err(|e| format!("reading from the browser: {e}"))?;
        let (fin, opcode, masked) = (head[0] & 0x80 != 0, head[0] & 0x0f, head[1] & 0x80 != 0);
        let len = match head[1] & 0x7f {
            126 => {
                let mut b = [0u8; 2];
                r.read_exact(&mut b).map_err(|e| e.to_string())?;
                u16::from_be_bytes(b) as u64
            }
            127 => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b).map_err(|e| e.to_string())?;
                u64::from_be_bytes(b)
            }
            n => n as u64,
        };
        let mut mask = [0u8; 4];
        if masked {
            r.read_exact(&mut mask).map_err(|e| e.to_string())?;
        }
        let mut payload = vec![0u8; len as usize];
        r.read_exact(&mut payload).map_err(|e| format!("reading from the browser: {e}"))?;
        if masked {
            payload.iter_mut().enumerate().for_each(|(i, b)| *b ^= mask[i % 4]);
        }
        match opcode {
            8 => return Err("the browser closed the connection".into()),
            9 | 10 => continue,
            _ => message.extend_from_slice(&payload),
        }
        if fin {
            return Ok(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        for n in [0, 5, 125, 126, 300, 70_000] {
            let payload: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
            let frame = client_frame(&payload, mask_key(n as u64));
            assert_eq!(frame[0], 0x81);
            assert!(frame[1] & 0x80 != 0, "client frames are masked");
            assert_eq!(read_message(&mut &frame[..]).unwrap(), payload);
        }
    }

    #[test]
    fn continuation_frames_join_and_pings_are_skipped() {
        // Unmasked server frames: "ab" (not final), a ping, "cd" (continuation, final).
        let bytes = [&[0x01, 2][..], b"ab", &[0x89, 0], &[0x80, 2], b"cd"].concat();
        assert_eq!(read_message(&mut &bytes[..]).unwrap(), b"abcd");
        assert!(read_message(&mut &[0x88u8, 0][..]).is_err(), "a close frame ends the connection");
    }
}
