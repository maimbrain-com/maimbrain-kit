//! API keys for generation providers: environment variables first, then
//! `mb keys set`, which saves them in `~/.config/maimbrain/keys.json`
//! (mode 0600; `%APPDATA%\maimbrain\keys.json` on Windows), next to the
//! `mb login` credentials and never inside a game or repository.
//! `MB_KEYS_FILE` points somewhere else (tests use it).

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;

/// (provider, env vars in order, how to get a key)
pub const PROVIDERS: &[(&str, &[&str], &str)] = &[
    ("gemini", &["GEMINI_API_KEY", "GOOGLE_API_KEY"], "create a key at https://aistudio.google.com/apikey; image and music models need billing on its project"),
    (
        "vertex",
        &["GOOGLE_CLOUD_PROJECT"],
        "the value is a Google Cloud project id with the Vertex AI API enabled; mb gets a token from `gcloud auth print-access-token` (or VERTEX_ACCESS_TOKEN)",
    ),
    ("openai", &["OPENAI_API_KEY"], "create a key at https://platform.openai.com/api-keys; GPT Image may need organization verification"),
    (
        "elevenlabs",
        &["ELEVENLABS_API_KEY", "XI_API_KEY"],
        "create a key at https://elevenlabs.io/app/settings/api-keys; the free plan includes a few sound effects a month",
    ),
    (
        "tripo",
        &["TRIPO_API_KEY"],
        "create a key at https://platform.tripo3d.ai (API Keys) and buy credits ($0.01 each, pay as you go); models made with free credits aren't yours to ship",
    ),
    ("meshy", &["MESHY_API_KEY"], "create a key at https://www.meshy.ai/developers/keys; the API needs a paid plan (Pro, $20 a month for 1000 credits)"),
];

fn keys_path() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("MB_KEYS_FILE") {
        return Ok(PathBuf::from(p));
    }
    Ok(crate::remote::config_dir()?.join("keys.json"))
}

fn load_file() -> BTreeMap<String, String> {
    keys_path().ok().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_file(keys: &BTreeMap<String, String>) -> Result<PathBuf, String> {
    let path = keys_path()?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(keys).unwrap()).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// The key for a provider and where it came from.
pub fn lookup(provider: &str) -> Option<(String, String)> {
    lookup_with(provider, &|k| std::env::var(k).ok(), &load_file())
}

pub fn lookup_with(provider: &str, env: &dyn Fn(&str) -> Option<String>, file: &BTreeMap<String, String>) -> Option<(String, String)> {
    let (_, vars, _) = PROVIDERS.iter().find(|p| p.0 == provider)?;
    for v in *vars {
        if let Some(k) = env(v).filter(|k| !k.trim().is_empty()) {
            return Some((k.trim().to_string(), (*v).to_string()));
        }
    }
    file.get(provider).filter(|k| !k.is_empty()).map(|k| (k.clone(), "mb keys".to_string()))
}

pub fn mask(k: &str) -> String {
    let chars: Vec<char> = k.chars().collect();
    if chars.len() <= 8 {
        return "…".into();
    }
    format!("{}…{}", chars[..4].iter().collect::<String>(), chars[chars.len() - 4..].iter().collect::<String>())
}

pub fn help_for(provider: &str) -> String {
    match PROVIDERS.iter().find(|p| p.0 == provider) {
        Some((name, vars, how)) => format!("set {} or run `mb keys set {name}` ({how})", vars.join(" or ")),
        None => format!("unknown provider {provider}"),
    }
}

#[derive(clap::Subcommand)]
pub enum KeysCmd {
    /// Save a provider's API key (read from stdin; not echoed in a terminal).
    Set {
        /// gemini, vertex (a Google Cloud project id), openai, elevenlabs, tripo or meshy.
        provider: String,
    },
    /// Show which providers have keys and where each comes from (keys masked).
    List,
    /// Forget a saved key (environment variables are untouched).
    Remove { provider: String },
}

pub fn run(cmd: KeysCmd) -> Result<(), String> {
    match cmd {
        KeysCmd::Set { provider } => {
            let Some((_, vars, url)) = PROVIDERS.iter().find(|p| p.0 == provider) else {
                return Err(format!("unknown provider {provider:?} (gemini, vertex, openai, elevenlabs, tripo, meshy)"));
            };
            let stdin = std::io::stdin();
            let tty = stdin.is_terminal();
            if tty {
                eprint!("{} for {provider} ({url}): ", if provider == "vertex" { "Google Cloud project id" } else { "API key" });
                let _ = std::io::stderr().flush();
            }
            let hidden = tty && cfg!(unix) && provider != "vertex" && stty(false);
            let mut line = String::new();
            let read = stdin.lock().read_line(&mut line);
            if hidden {
                stty(true);
                eprintln!();
            }
            read.map_err(|e| e.to_string())?;
            let key = line.trim().to_string();
            if key.is_empty() {
                return Err("nothing entered".into());
            }
            let mut keys = load_file();
            keys.insert(provider.clone(), key.clone());
            let path = save_file(&keys)?;
            eprintln!("saved {provider} key {} in {}", mask(&key), path.display());
            if let Some(v) = vars.iter().find(|v| std::env::var(v).is_ok()) {
                eprintln!("note: {v} is set in your environment and takes precedence");
            }
            Ok(())
        }
        KeysCmd::List => {
            for (name, _, url) in PROVIDERS {
                match lookup(name) {
                    Some((k, from)) => {
                        let shown = if *name == "vertex" { k } else { mask(&k) };
                        eprintln!("✓ {name:11} {shown}  (from {from})");
                    }
                    None => eprintln!("· {name:11} not set  →  {}", help_for(name).replace(&format!(" ({url})"), "")),
                }
            }
            eprintln!("  mock        always available (offline, for trying the pipeline: --provider mock)");
            Ok(())
        }
        KeysCmd::Remove { provider } => {
            let mut keys = load_file();
            if keys.remove(&provider).is_none() {
                return Err(format!("no saved key for {provider}"));
            }
            save_file(&keys)?;
            eprintln!("removed the saved {provider} key");
            Ok(())
        }
    }
}

/// Turns terminal echo on or off (unix); false if it couldn't.
fn stty(echo: bool) -> bool {
    std::process::Command::new("stty").arg(if echo { "echo" } else { "-echo" }).stdin(std::process::Stdio::inherit()).status().is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_wins_over_file_and_first_var_wins() {
        let mut file = BTreeMap::new();
        file.insert("gemini".to_string(), "file-key-123456".to_string());
        let env = |k: &str| match k {
            "GOOGLE_API_KEY" => Some("google-key-123456".to_string()),
            _ => None,
        };
        assert_eq!(lookup_with("gemini", &env, &file), Some(("google-key-123456".into(), "GOOGLE_API_KEY".into())));
        let none = |_: &str| None;
        assert_eq!(lookup_with("gemini", &none, &file), Some(("file-key-123456".into(), "mb keys".into())));
        assert_eq!(lookup_with("openai", &none, &file), None);
        assert_eq!(lookup_with("nope", &none, &file), None);
    }

    #[test]
    fn masks_keys() {
        assert_eq!(mask("AIzaSyA-very-secret-f3k9"), "AIza…f3k9");
        assert_eq!(mask("short"), "…");
    }
}
