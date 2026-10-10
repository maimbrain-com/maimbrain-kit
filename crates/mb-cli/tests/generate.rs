//! End-to-end runs of `mb art`, `mb music`, `mb sfx`, `mb model`, `mb regen` and
//! `mb keys` with the offline mock provider. No network: keys come from a
//! throwaway file and no real provider is ever selected.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mb-gen-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn mb(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mb"))
        .args(args)
        .current_dir(dir)
        .env("MB_KEYS_FILE", dir.join("keys.json"))
        // Signed out: no credentials, so nothing asks a server about Maimbrain's keys.
        .env("MB_CONFIG_DIR", dir.join("config"))
        .env_remove("MB_KEYS_SOURCE")
        .env_remove("MB_PROVIDER")
        .env_remove("GEMINI_API_KEY")
        .env_remove("GOOGLE_API_KEY")
        .env_remove("GOOGLE_CLOUD_PROJECT")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ELEVENLABS_API_KEY")
        .env_remove("XI_API_KEY")
        .env_remove("TRIPO_API_KEY")
        .env_remove("MESHY_API_KEY")
        .env_remove("MB_MAX_COST")
        .env("MB_GEN_DATE", "2026-10-07T00:00:00Z")
        .output()
        .unwrap()
}

fn ok(o: &Output) -> String {
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(o.status.success(), "mb failed:\n{err}");
    err
}

fn game(dir: &Path) -> PathBuf {
    let g = dir.join("g");
    std::fs::create_dir_all(g.join("assets")).unwrap();
    std::fs::write(g.join("manifest.toml"), "abi = 0\nid = \"com.example.g\"\nname = \"G\"\nversion = \"0.1.0\"\ncreator = \"@t\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nstdlib = { mb2d = 1 }\n").unwrap();
    g
}

fn sidecar(p: &Path) -> serde_json::Value {
    let mut s = p.as_os_str().to_owned();
    s.push(".gen.json");
    serde_json::from_str(&std::fs::read_to_string(PathBuf::from(s)).unwrap()).unwrap()
}

#[test]
fn sprite_with_style_sidecar_and_reprocess() {
    let d = tmp("sprite");
    let g = game(&d);
    ok(&mb(&d, &["art", "init", "g"]));
    assert!(g.join("art/style.toml").exists() && g.join("art/.gitignore").exists());
    ok(&mb(&d, &["art", "sprite", "g", "hero", "a round moth", "--provider", "mock", "--res", "0.5K", "--seed", "7"]));
    let png = g.join("assets/hero.png");
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[16..24], &[0, 0, 0, 128, 0, 0, 0, 128], "128×128 by default");
    let sc = sidecar(&png);
    assert_eq!(sc["provider"], "mock");
    assert_eq!(sc["seed"], 7);
    assert_eq!(sc["prompt"], "a round moth");
    assert_eq!(sc["date"], "2026-10-07T00:00:00Z");
    assert!(sc["full_prompt"].as_str().unwrap().contains("Use only these colors"), "style applied");
    assert_eq!(sc["style"]["path"], "art/style.toml");
    assert!(g.join(sc["raw"].as_str().unwrap()).exists());
    assert!(g.join("art/preview/hero.png").exists());
    // The sidecar records the key color the model was asked for.
    let key = sc["recipe"]["key"].as_str().unwrap().to_string();
    assert!(sc["full_prompt"].as_str().unwrap().contains(&format!("({key})")), "{key}");
    // Reprocessing the raw output gives the same bytes, without a provider.
    ok(&mb(&d, &["regen", "g/assets/hero.png", "--reprocess"]));
    assert_eq!(std::fs::read(&png).unwrap(), bytes);
    // --tolerance and --key are taken and recorded.
    ok(&mb(&d, &["regen", "g/assets/hero.png", "--reprocess", "--tolerance", "0.12", "--key", &key]));
    assert_eq!(sidecar(&png)["recipe"]["tolerance"], 0.12);
    ok(&mb(&d, &["regen", "g/assets/hero.png", "--reprocess", "--tolerance", "0.09"]));
    assert_eq!(std::fs::read(&png).unwrap(), bytes);
    // Same seed, same provider: same result.
    ok(&mb(&d, &["regen", "g/assets/hero.png"]));
    assert_eq!(std::fs::read(&png).unwrap(), bytes);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn variants_pick_atlas_and_check() {
    let d = tmp("variants");
    let g = game(&d);
    let e = ok(&mb(&d, &["art", "sprite", "g", "enemy", "a spiky blob", "--provider", "mock", "--res", "0.5K", "--variants", "3", "--size", "64x64"]));
    assert!(e.contains("mb art pick"));
    assert!(g.join("art/variants/enemy-sheet.png").exists());
    assert!(!g.join("assets/enemy.png").exists());
    ok(&mb(&d, &["art", "pick", "g", "enemy", "2"]));
    assert!(g.join("assets/enemy.png").exists());
    ok(&mb(&d, &["art", "ui", "g", "button", "a round button", "--provider", "mock", "--res", "0.5K", "--size", "96x48"]));
    ok(&mb(&d, &["art", "atlas", "g", "sprites"]));
    let rs = std::fs::read_to_string(g.join("src/sprites.rs")).unwrap();
    assert!(rs.contains("pub const ENEMY: [f32; 4]") && rs.contains("pub const BUTTON: [f32; 4]"), "{rs}");
    let e = String::from_utf8_lossy(&mb(&d, &["art", "check", "g"]).stderr).to_string();
    assert!(e.contains("mock placeholder") && e.contains("mb art atlas of 2 images"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn icon_background_frames_layers_tile() {
    let d = tmp("kinds");
    let g = game(&d);
    for args in [
        vec!["art", "icon", "g", "the hero's face"],
        vec!["art", "background", "g", "sky", "a dusk sky", "--size", "180x320"],
        vec!["art", "frames", "g", "walk", "walking", "--frames", "4", "--size", "48x48"],
        vec!["art", "layers", "g", "hills", "hills", "--count", "2", "--size", "200x100"],
        vec!["art", "tile", "g", "stone", "stones", "--grid", "8x8"],
    ] {
        let mut a = args.clone();
        a.extend(["--provider", "mock", "--res", "0.5K"]);
        ok(&mb(&d, &a));
    }
    let dims = |p: &str| {
        let b = std::fs::read(g.join(p)).unwrap();
        (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()))
    };
    assert_eq!(dims("icon.png"), (256, 256));
    assert_eq!(dims("assets/sky.png"), (180, 320));
    assert_eq!(dims("assets/walk.png"), (192, 48));
    assert_eq!(dims("assets/hills_0.png"), (200, 100));
    assert_eq!(dims("assets/hills_1.png"), (200, 100));
    assert_eq!(dims("assets/stone.png"), (32, 32), "8×8 art pixels at the default pixel_scale 4");
    // The near layer used the far layer as a style reference.
    assert_eq!(sidecar(&g.join("assets/hills_1.png"))["references"][0]["path"], "assets/hills_0.png");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn talking_portrait_sheet_layout_and_reprocess() {
    let d = tmp("portrait");
    let g = game(&d);
    let dims = |p: &Path| {
        let b = std::fs::read(p).unwrap();
        (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()))
    };
    // Default: neutral plus the blink row, 4 × 2 cells of 128×128.
    ok(&mb(&d, &["art", "portrait", "g", "ace", "a cocky mech pilot in a dented orange helmet", "--provider", "mock", "--res", "0.5K"]));
    assert_eq!(dims(&g.join("assets/ace.png")), (512, 256));
    let p = sidecar(&g.join("assets/ace.png"))["result"]["portrait"].clone();
    assert_eq!(p, serde_json::json!({ "cols": 4, "rows": 2, "cell": [128, 128], "expressions": ["neutral"], "blink_row": 1, "mouths": ["closed", "slightly open", "open", "wide open"] }));
    // Named expressions, one row each, in a pixel-art sheet.
    let e = ok(&mb(
        &d,
        &["art", "portrait", "g", "rival", "a grumpy pilot with goggles", "--expressions", "smug,angry,scared", "--grid", "16x16", "--provider", "mock", "--res", "0.5K", "--seed", "4"],
    ));
    assert!(e.contains("ui::Portrait") && e.contains("row = 0 neutral, 1 smug, 2 angry, 3 scared, 4 blink"), "{e}");
    let png = g.join("assets/rival.png");
    assert_eq!(dims(&png), (256, 320), "4 × 5 cells of 16×16 art pixels at pixel_scale 4");
    let sc = sidecar(&png);
    assert_eq!(sc["result"]["portrait"]["expressions"], serde_json::json!(["neutral", "smug", "angry", "scared"]));
    assert_eq!(sc["result"]["portrait"]["blink_row"], 4);
    let prompt = sc["full_prompt"].as_str().unwrap();
    assert!(prompt.contains("4 columns and 5 rows") && prompt.contains("row 2 smug") && prompt.contains("eyes fully closed"), "{prompt}");
    let bytes = std::fs::read(&png).unwrap();
    ok(&mb(&d, &["regen", "g/assets/rival.png", "--reprocess"]));
    assert_eq!(std::fs::read(&png).unwrap(), bytes, "reprocessing keeps the layout");
    // Cells are square; blink is always there.
    assert!(!mb(&d, &["art", "portrait", "g", "x", "y", "--size", "64x96", "--provider", "mock"]).status.success());
    assert!(!mb(&d, &["art", "portrait", "g", "x", "y", "--expressions", "blink", "--provider", "mock"]).status.success());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn sky_panorama_is_a_seamless_2_to_1_jpeg_with_calm_poles() {
    let d = tmp("sky");
    let g = game(&d);
    let err = ok(&mb(&d, &["art", "sky", "g", "night", "a moonless night over distant rooftops", "--provider", "mock", "--res", "1K", "--size", "512x256"]));
    assert!(err.contains("aspect 21:9"), "{err}");
    let jpg = std::fs::read(g.join("assets/night.jpg")).unwrap();
    assert!(jpg.starts_with(&[0xff, 0xd8]) && jpg.len() < 800 * 1024);
    let sc = sidecar(&g.join("assets/night.jpg"));
    assert_eq!(sc["recipe"]["kind"], "sky");
    assert_eq!(sc["result"]["projection"], "equirectangular");
    assert_eq!(sc["result"]["size"], serde_json::json!([512, 256]));
    assert!(sc["full_prompt"].as_str().unwrap().contains("equirectangular panorama"));
    assert!(sc["note"].as_str().unwrap().contains("mock"));
    // Reprocessing redoes it from the raw output, offline.
    ok(&mb(&d, &["regen", "g/assets/night.jpg", "--reprocess"]));
    let check = String::from_utf8_lossy(&mb(&d, &["art", "check", "g"]).stderr).to_string();
    assert!(check.contains("assets/night.jpg") && check.contains("512×256") && check.contains("mock placeholder"), "{check}");
    // Skies are 2:1 and fit a texture.
    let o = mb(&d, &["art", "sky", "g", "x", "a sky", "--provider", "mock", "--size", "512x512"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("2:1"));
    let o = mb(&d, &["art", "sky", "g", "x", "a sky", "--provider", "mock", "--size", "4096x2048"]);
    assert!(!o.status.success());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn ip_refusal_cost_cap_and_dry_run_without_keys() {
    let d = tmp("guards");
    game(&d);
    let o = mb(&d, &["art", "sprite", "g", "x", "Pikachu in a hat", "--provider", "mock"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("someone else's"));
    ok(&mb(&d, &["art", "sprite", "g", "x", "Pikachu in a hat", "--provider", "mock", "--allow-name", "pikachu", "--res", "0.5K"]));
    // No key: a dry run still shows the prompt and the price of the default model.
    let e = ok(&mb(&d, &["art", "sprite", "g", "y", "a lantern", "--dry-run"]));
    assert!(e.contains("gemini-nano-banana-2.1") && e.contains("$0.0336") && e.contains("prompt:"), "{e}");
    // The spending cap.
    let o = mb(&d, &["art", "sprite", "g", "y", "a lantern", "--dry-run", "--variants", "4", "--max-cost", "0.10"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("over the $0.10 limit"));
    // Without a dry run and without a key, a clear setup message.
    let o = mb(&d, &["art", "sprite", "g", "y", "a lantern"]);
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success() && e.contains("GEMINI_API_KEY") && e.contains("ask the Maimbrain team"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn keys_are_saved_outside_the_game_and_masked() {
    let d = tmp("keys");
    let mut c = Command::new(env!("CARGO_BIN_EXE_mb"))
        .args(["keys", "set", "elevenlabs"])
        .env("MB_KEYS_FILE", d.join("keys.json"))
        .env_remove("ELEVENLABS_API_KEY")
        .env_remove("XI_API_KEY")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    c.stdin.take().unwrap().write_all(b"sk_test_1234567890abcdef\n").unwrap();
    assert!(c.wait().unwrap().success());
    let o = mb(&d, &["keys", "list"]);
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(e.contains("sk_t…cdef") && !e.contains("1234567890"), "{e}");
    ok(&mb(&d, &["keys", "remove", "elevenlabs"]));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn music_and_sfx() {
    let d = tmp("sound");
    let g = game(&d);
    let o = mb(&d, &["music", "g", "music", "bouncy marimba", "--provider", "mock", "--bars", "4", "--rate", "22050", "--seed", "3"]);
    let e = ok(&o);
    assert!(e.contains("4 bars of 4/4") && e.contains("(clean)"), "{e}");
    let sc = sidecar(&g.join("assets/music.ogg"));
    assert_eq!(sc["result"]["bars"], 4);
    assert!(sc["result"]["seam_ratio"].as_f64().unwrap() < 4.0);
    assert!((sc["result"]["loud_rms_db"].as_f64().unwrap() + 20.0).abs() < 1.5);
    ok(&mb(&d, &["music", "g", "theme", "synth", "--provider", "mock", "--layers", "--rate", "22050"]));
    assert!(g.join("assets/theme_base.ogg").exists() && g.join("assets/theme_hi.ogg").exists());
    ok(&mb(&d, &["sfx", "g", "chime", "a glass chime", "--provider", "mock", "--seconds", "0.3"]));
    let sc = sidecar(&g.join("assets/chime.ogg"));
    assert!(sc["result"]["seconds"].as_f64().unwrap() < 0.45, "silence trimmed");
    assert!(sc["result"]["phone_loss_db"].as_f64().unwrap() <= 0.0);
    ok(&mb(&d, &["regen", "g/assets/chime.ogg", "--reprocess"]));
    // --brighten on a take a phone plays fine leaves it alone, and says so; it's in the recipe.
    let e = ok(&mb(&d, &["regen", "g/assets/chime.ogg", "--reprocess", "--brighten"]));
    assert!(e.contains("--brighten: not needed"), "{e}");
    assert_eq!(sidecar(&g.join("assets/chime.ogg"))["recipe"]["brighten"], true);
    // An edit after generation, recorded: mb art check names it.
    let chime = g.join("assets/chime.ogg");
    let mut bytes = std::fs::read(&chime).unwrap();
    bytes.extend_from_slice(b"edit");
    std::fs::write(&chime, &bytes).unwrap();
    let e = ok(&mb(&d, &["art", "check", "g"]));
    assert!(e.contains("edited since it was generated"), "{e}");
    ok(&mb(&d, &["art", "edited", "g/assets/chime.ogg", "--tool", "tools/bright.py", "--note", "a click on top", "--", "--shelf", "6"]));
    let e = ok(&mb(&d, &["art", "check", "g"]));
    assert!(e.contains("edited after generation: tools/bright.py --shelf 6 (a click on top)"), "{e}");
    let sc = sidecar(&chime);
    assert_eq!(sc["derived"][0]["args"], serde_json::json!(["--shelf", "6"]));
    assert_ne!(sc["derived"][0]["output_sha256"], sc["output_sha256"]);
    assert!(!mb(&d, &["art", "edited", "g/assets/chime.ogg", "--tool", "hand"]).status.success(), "nothing new to record");
    // Regenerating replaces the file and says the edit is gone.
    let e = ok(&mb(&d, &["regen", "g/assets/chime.ogg", "--reprocess"]));
    assert!(e.contains("was edited after generation (tools/bright.py --shelf 6"), "{e}");
    assert!(sidecar(&chime).get("derived").is_none());
    // The tension kit: aligned loops and one-shots, synthesized in the loop's key.
    let e = ok(&mb(
        &d,
        &["music", "g", "chase", "surf rock", "--provider", "mock", "--layers", "base,hi,tension", "--bars", "4", "--rate", "22050", "--seed", "3"],
    ));
    assert!(e.contains("tension kit: synthesized in") && e.contains("key: ~"), "{e}");
    let base = sidecar(&g.join("assets/chase_base.ogg"));
    for (layer, looped) in [("tension", true), ("tick", true), ("heartbeat", false), ("sting", false)] {
        let sc = sidecar(&g.join(format!("assets/chase_{layer}.ogg")));
        assert_eq!(sc["provider"], "mb", "{layer}");
        assert_eq!(sc["cost_usd"], 0.0);
        assert_eq!(sc["result"]["layer"], layer);
        assert_eq!(sc["result"]["key"], base["result"]["key"]);
        if looped {
            assert_eq!(sc["result"]["loop_samples"], base["result"]["loop_samples"], "{layer} is as long as the base");
            assert!(sc["result"]["seam_ratio"].as_f64().unwrap() < 4.0);
        }
    }
    // A given key wins; bad layer lists are refused.
    ok(&mb(&d, &["regen", "g/assets/chase_tension.ogg", "--reprocess"]));
    ok(&mb(&d, &["music", "g", "calm", "harp", "--provider", "mock", "--layers", "tension", "--key", "F#m", "--rate", "22050"]));
    assert_eq!(sidecar(&g.join("assets/calm_tension.ogg"))["result"]["key_given"], "F# minor");
    assert!(g.join("assets/calm.ogg").exists(), "--layers tension keeps the full mix");
    for bad in ["base", "hi,tension", "drums"] {
        assert!(!mb(&d, &["music", "g", "x", "harp", "--provider", "mock", "--layers", bad]).status.success(), "{bad}");
    }
    // A recipe set: a mech's foley in one command, loops where they should loop.
    let e = ok(&mb(&d, &["sfx", "g", "--pack", "mech", "--provider", "mock", "--prefix", "rook_", "--flavor", "a rusty junkyard robot"]));
    for (s, looped) in [("stomp", false), ("servo", true), ("thruster", true), ("hiss", false), ("impact", false), ("tear", false)] {
        let sc = sidecar(&g.join(format!("assets/rook_{s}.ogg")));
        assert_eq!(sc["recipe"]["looping"], looped, "{s}");
        assert!(sc["prompt"].as_str().unwrap().contains("rusty junkyard robot"), "{s}");
        assert!(e.contains(&format!("rook_{s}:")), "{e}");
    }
    assert!(!mb(&d, &["sfx", "g", "x", "--pack", "mech"]).status.success(), "a pack takes no name");
    // A different song per zone: each its own graph, tension kit and stinger, and a setlist.
    let e = ok(&mb(
        &d,
        &["music", "g", "brawl", "for a mech duel", "--provider", "mock", "--zone", "harbor=bluesy hard rock, 128 bpm", "--zone", "neon=flamenco-tango, 112 bpm", "--rate", "22050", "--seed", "3"],
    ));
    assert!(e.contains("2 songs made (harbor: 128 BPM") && e.contains("each in its own key and tempo on purpose"), "{e}");
    let list: serde_json::Value = serde_json::from_slice(&std::fs::read(g.join("assets/brawl.songs.json")).unwrap()).unwrap();
    assert_eq!(list["format"], "maimbrain-songs");
    for (i, (zone, bpm)) in [("harbor", 128.0), ("neon", 112.0)].into_iter().enumerate() {
        let s = &list["songs"][i];
        assert_eq!(s["zone"], zone);
        assert!((s["bpm"].as_f64().unwrap() - bpm).abs() < 1.0, "{s}");
        for f in ["music.json", "take.ogg", "tension.ogg", &format!("into_{zone}.ogg")] {
            assert!(g.join(format!("assets/brawl_{zone}_{f}")).exists() || g.join(format!("assets/brawl_{zone}.{f}")).exists(), "{zone} {f}");
        }
        let take = sidecar(&g.join(format!("assets/brawl_{zone}_take.ogg")));
        assert!(take["prompt"].as_str().unwrap().contains("for a mech duel"), "the shared prompt is in each");
    }
    // A later run adds to the list; a zone the filter refuses is reported and the one after it still made.
    let o = mb(
        &d,
        &["music", "g", "brawl", "--provider", "mock", "--zone", "yard=stomping metal, greedy swagger mock-refuse", "--zone", "dock=sea shanty, 100 bpm", "--rate", "22050"],
    );
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success(), "a failed zone fails the run: {e}");
    assert!(e.contains("1 of 2 zones failed: yard") && e.contains("\"stomping\", \"greedy\", \"swagger\""), "{e}");
    assert!(e.contains("mb music g brawl --provider mock --zone yard=\"heavy metal, brash confident mock-refuse\""), "{e}");
    let list: serde_json::Value = serde_json::from_slice(&std::fs::read(g.join("assets/brawl.songs.json")).unwrap()).unwrap();
    let zones: Vec<&str> = list["songs"].as_array().unwrap().iter().map(|s| s["zone"].as_str().unwrap()).collect();
    assert_eq!(zones, ["harbor", "neon", "dock"], "merged, not rewritten");
    assert!(!mb(&d, &["music", "g", "x", "--zone", "a=b", "--zone", "c=d", "--zones", "e: f; g: h"]).status.success());
    // No sfx key and no --provider: the procedural path is suggested.
    let o = mb(&d, &["sfx", "g", "boom", "an explosion"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("sfx.py"));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn music_in_sections_and_layers_for_a_take_you_have() {
    let d = tmp("sections");
    let g = game(&d);
    // Sections ask for Lyria 3.5 (Lyria 3 Clip is 30 s) with a timestamped line each.
    let e = ok(&mb(&d, &["music", "g", "x", "taiko chase", "--zones", "lantern town: warm; bamboo grove: hollow wood", "--bpm", "120", "--dry-run"]));
    assert!(e.contains("lyria-3.5") && e.contains("[0:00 - 0:22] Lantern Town: warm.") && e.contains("× 2"), "{e}");
    let zones = "lantern town: warm lanterns; bamboo grove: hollow wood";
    let args = ["music", "g", "run", "taiko chase", "--provider", "mock", "--zones", zones, "--variations", "2", "--bars", "4", "--bpm", "120"];
    let e = ok(&mb(&d, &[&args[..], &["--layers", "base,hi,tension", "--rate", "22050", "--seed", "3", "--loops"]].concat()));
    assert!(e.contains("4 loops in one key, tempo and bar grid") && e.contains("music::Director"), "{e}");
    let a = g.join("assets");
    // The music graph: the take as base and high layers, its sections on the piece's bars, its zones
    // (a zone and its variation are one), and the files a game loads, named from the game's root.
    let graph: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(a.join("run.music.json")).unwrap()).unwrap();
    assert_eq!(graph["format"], "maimbrain-music-graph");
    assert_eq!(graph["files"]["base"], "assets/run_take_base.ogg");
    assert_eq!(graph["files"]["tension"]["layer"], "assets/run_tension.ogg");
    assert_eq!(graph["zones"].as_array().unwrap().iter().map(|z| z["name"].as_str().unwrap()).collect::<Vec<_>>(), ["lantern_town", "bamboo_grove"]);
    assert_eq!(graph["zones"][1]["stinger"]["file"], "assets/run_into_bamboo_grove_1.ogg");
    let secs = graph["sections"].as_array().unwrap();
    assert_eq!(secs.len(), 4);
    for (i, s) in secs.iter().enumerate() {
        let (entry, exit) = (s["entry_bar"].as_u64().unwrap(), s["exit_bar"].as_u64().unwrap());
        assert!(exit > entry && (i == 0 || secs[i - 1]["exit_bar"] == s["entry_bar"]), "{s}");
        assert_eq!(s["exit_cues"].as_array().unwrap().last().unwrap(), &s["exit_bar"]);
        let seg = &s["segment"];
        assert!(seg["pre_entry_seconds"].as_f64() <= seg["entry_seconds"].as_f64() && seg["post_exit_seconds"].as_f64() > seg["exit_seconds"].as_f64(), "{seg}");
    }
    assert_eq!(graph["transitions"].as_array().unwrap().len(), 2);
    assert!(graph["transitions"].as_array().unwrap().iter().any(|t| t["from"] == 0 && t["to"] == 1 && t["take_runs_into_it"] == true));
    for l in graph["links"].as_array().unwrap() {
        assert!(l["score"].as_f64().unwrap() >= 0.65 && l["from"] != l["to"], "{l}");
    }
    let take = sidecar(&a.join("run_take_base.ogg"));
    assert_eq!(take["result"]["graph"], "assets/run.music.json");
    assert!(sidecar(&a.join("run.music.json"))["output_sha256"].is_string());
    let first = sidecar(&a.join("run_lantern_town_1_base.ogg"));
    let r = &first["result"];
    assert!((r["bpm"].as_f64().unwrap() - 120.0).abs() < 0.5, "{r}");
    assert_eq!(r["downbeat_seconds"], 0.01_f32 as f64);
    assert_eq!(first["recipe"]["section_seconds"], 18.0, "4 bars + 3, at least 18 s");
    assert!(r["mismatches"].as_array().unwrap().is_empty(), "{r}");
    for zone in ["lantern_town", "bamboo_grove"] {
        for k in 1..=2 {
            for layer in ["base", "hi"] {
                let sc = sidecar(&a.join(format!("run_{zone}_{k}_{layer}.ogg")));
                assert_eq!(sc["result"]["loop_samples"], r["loop_samples"], "{zone}_{k} is as long as the others");
                assert_eq!(sc["result"]["key"], r["key"]);
                let r = &sc["result"];
                assert!(r["seam_ratio"].as_f64().unwrap() < 4.0 || r["seam_jump_db"].as_f64().unwrap() < -40.0, "{r}");
            }
        }
        // A stinger into each zone's first variation, landing two beats in.
        let st = sidecar(&a.join(format!("run_into_{zone}_1.ogg")));
        assert_eq!(st["model"], "transition-stinger");
        assert!((st["result"]["land_seconds"].as_f64().unwrap() - 1.0).abs() < 0.01);
        assert!(!a.join(format!("run_into_{zone}_2.ogg")).exists());
    }
    for kit in ["tension", "tick"] {
        assert_eq!(sidecar(&a.join(format!("run_{kit}.ogg")))["result"]["loop_samples"], r["loop_samples"]);
    }
    // Reprocessing any of them redoes the set from the same take: the same sound (Ogg's stream serial differs).
    let before = sidecar(&a.join("run_bamboo_grove_2_hi.ogg"))["result"].clone();
    let e = ok(&mb(&d, &["regen", "g/assets/run_bamboo_grove_2_hi.ogg", "--reprocess"]));
    assert!(e.contains("reprocessing art/raw/run-3.wav"), "{e}");
    assert_eq!(sidecar(&a.join("run_bamboo_grove_2_hi.ogg"))["result"], before);
    // One grid for the piece, in every file; every loop starts on one of its bar lines,
    // with a bar of crossfade baked in and its join measured.
    let grid = r["grid"].clone();
    assert!(grid["sections_agreeing"].is_string() && grid["downbeat_in_take_seconds"].is_number(), "{r}");
    let (down, bar) = (grid["downbeat_in_take_seconds"].as_f64().unwrap(), grid["bar_seconds"].as_f64().unwrap());
    assert!((bar - 2.0).abs() < 0.01, "{grid}");
    for zone in ["lantern_town", "bamboo_grove"] {
        for k in 1..=2 {
            let r = &sidecar(&a.join(format!("run_{zone}_{k}_hi.ogg")))["result"];
            assert_eq!(r["grid"], grid);
            let at = (r["loop_start_seconds"].as_f64().unwrap() - down) / bar;
            assert!((at - at.round()).abs() < 0.001, "{zone}_{k} starts {at} bars from the piece's 1");
            assert!(r["loop_match"].as_f64().unwrap() > 0.5 && r["phrase_bars"].as_u64().is_some(), "{r}");
            assert!((r["xfade_seconds"].as_f64().unwrap() - 2.0).abs() < 0.01, "{r}");
        }
    }
    let sw = sidecar(&a.join("run_swell.ogg"));
    assert!(sw["model"] == "transition-swell" && (sw["result"]["land_seconds"].as_f64().unwrap() - 0.5).abs() < 0.01, "{sw}");
    // --downbeat moves the piece's grid, from the take you have.
    let e = ok(&mb(&d, &["music", "g", "run", "--downbeat", "1"]));
    assert!(e.contains("reprocessing art/raw/run-3.wav") && e.contains("moved +1 beats (--downbeat)"), "{e}");
    let moved = sidecar(&a.join("run_lantern_town_1_base.ogg"));
    let shift = (moved["result"]["grid"]["downbeat_in_take_seconds"].as_f64().unwrap() - down).rem_euclid(bar);
    assert!((shift - 0.5).abs() < 0.001 && moved["recipe"]["downbeat_shift"] == 1.0, "{shift}");
    // --fix-loops (1: every section) asks for a clip of its own for each (the mock's, free here), fitted
    // to the piece and used where it joins better; --downbeat 0 puts the grid back.
    let e = ok(&mb(&d, &["music", "g", "run", "--reprocess", "--fix-loops", "1", "--downbeat", "0"]));
    assert!(e.contains("--fix-loops: 4 join under 1.00") && e.contains("asking mock mock-music"), "{e}");
    assert_eq!(e.matches("its own clip").count() + e.matches("its clip (").count(), 4, "{e}");
    assert_eq!(sidecar(&a.join("run_lantern_town_1_base.ogg"))["result"]["grid"], grid);
    let raws: Vec<String> = std::fs::read_dir(g.join("art/raw")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    assert!(
        ["lantern_town_1", "lantern_town_2", "bamboo_grove_1", "bamboo_grove_2"].iter().all(|s| raws.iter().any(|f| f.starts_with(&format!("run_{s}-")))),
        "{raws:?}"
    );
    let clips = sidecar(&a.join("run_lantern_town_1_base.ogg"))["recipe"]["section_clips"].clone();
    let used = clips.as_object().map_or(0, |m| m.len());
    // --reprocess reuses the clips that were used; --audition renders the graph's jumps and every
    // change between zones the Director makes into one file, spliced as it splices them.
    let e = ok(&mb(&d, &["music", "g", "run", "--reprocess", "--audition"]));
    assert_eq!(e.matches("its own clip").count() + e.matches("its clip (").count(), used, "{e}");
    assert!(e.contains("audition of run's music graph") && e.contains("lantern_town → bamboo_grove") && e.contains("bamboo_grove → lantern_town"), "{e}");
    assert!(e.contains("the take's own transition"), "{e}");
    assert!(g.join("art/raw/run-audition.ogg").exists());
    // With no graph (music made before it), each loop's join and every switch between the loops.
    std::fs::remove_file(a.join("run.music.json")).unwrap();
    let e = ok(&mb(&d, &["music", "g", "run", "--audition"]));
    assert!(e.contains("4 joins and 6 switches") && e.contains("lantern_town_1 → bamboo_grove_1") && e.contains("its stinger"), "{e}");
    assert!(e.contains("bamboo_grove_2 → bamboo_grove_1") && e.contains("a one-bar crossfade"), "{e}");
    // --variations alone: one zone, the take and its graph; with --loops, v1…vN too.
    ok(&mb(&d, &["music", "g", "theme", "synth", "--provider", "mock", "--variations", "2", "--bars", "4", "--bpm", "110", "--rate", "22050", "--retries", "0"]));
    let graph: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(a.join("theme.music.json")).unwrap()).unwrap();
    assert_eq!(graph["zones"].as_array().unwrap().len(), 1);
    assert_eq!(graph["zones"][0]["name"], "theme");
    assert!(a.join("theme_take.ogg").exists() && !a.join("theme_v1.ogg").exists() && !a.join("theme_into_v2.ogg").exists());
    ok(&mb(&d, &["music", "g", "theme", "--reprocess", "--loops"]));
    assert!((1..=2).all(|k| a.join(format!("theme_v{k}.ogg")).exists()) && !a.join("theme_into_v2.ogg").exists());

    // The tension kit for a track you already have (a creator's journal, F1): from the
    // take in art/raw/, no new take, no sidecar editing.
    ok(&mb(&d, &["music", "g", "calm", "harp", "--provider", "mock", "--bars", "4", "--rate", "22050", "--seed", "4"]));
    let mix = sidecar(&a.join("calm.ogg"))["result"]["loud_rms_db"].clone();
    let e = ok(&mb(&d, &["music", "g", "calm", "--reprocess", "--layers", "tension"]));
    assert!(e.contains("reprocessing art/raw/calm-4.wav (no provider call)"), "{e}");
    assert!(a.join("calm_tension.ogg").exists() && a.join("calm_sting.ogg").exists());
    assert_eq!(sidecar(&a.join("calm.ogg"))["result"]["loud_rms_db"], mix, "the same take, the same full mix");
    assert_eq!(sidecar(&a.join("calm.ogg"))["recipe"]["tension"], true);
    // mb regen --add-layers does the same from any of its files.
    let e = ok(&mb(&d, &["regen", "g/assets/calm_tension.ogg", "--add-layers", "base,hi"]));
    assert!(e.contains("new layers from the take you have"), "{e}");
    assert!(a.join("calm_base.ogg").exists() && a.join("calm_hi.ogg").exists());
    let rec = &sidecar(&a.join("calm_base.ogg"))["recipe"];
    assert!(rec["layers"] == true && rec["tension"] == true && rec["seed"] == 4, "{rec}");
    assert!(!mb(&d, &["regen", "g/assets/calm_base.ogg", "--add-layers", "drums"]).status.success());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn models_regen_variants_and_check() {
    let d = tmp("model");
    let g = game(&d);
    let args = ["model", "g", "koi", "--prompt", "a stylized koi", "--provider", "mock", "--animate", "swim", "--polys", "1500", "--texture-size", "128", "--glow", "2", "--seed", "5"];
    let e = ok(&mb(&d, &args));
    assert!(e.contains("Swim") && e.contains("KHR_materials_specular"), "{e}");
    let glb = g.join("assets/koi.glb");
    let bytes = std::fs::read(&glb).unwrap();
    assert_eq!(&bytes[..4], b"glTF");
    let sc = sidecar(&glb);
    assert_eq!(sc["provider"], "mock");
    assert_eq!(sc["recipe"]["kind"], "model");
    assert_eq!(sc["recipe"]["origin"], "center", "a fish is centered");
    assert!(sc["result"]["triangles"].as_u64().unwrap() <= 1500);
    assert_eq!(sc["result"]["texture_sizes"][0], serde_json::json!([128, 128]));
    assert_eq!(sc["result"]["clips"][0]["name"], "Swim");
    assert_eq!(sc["result"]["rigged"], true);
    // Quantized by default, in fewer bytes than floats.
    assert_eq!(sc["result"]["compressed"], true);
    assert!(sc["result"]["bytes"].as_u64() < sc["result"]["bytes_uncompressed"].as_u64(), "{}", sc["result"]);
    assert!(String::from_utf8_lossy(&bytes).contains("KHR_mesh_quantization"));
    assert!(g.join(sc["raw"].as_str().unwrap()).exists());
    assert!(g.join(sc["result"]["raw_clips"][0]["path"].as_str().unwrap()).exists());
    assert!(g.join("art/preview/koi.png").exists());
    // Reprocessing the raw output gives the same bytes; so does the same seed.
    ok(&mb(&d, &["regen", "g/assets/koi.glb", "--reprocess"]));
    assert_eq!(std::fs::read(&glb).unwrap(), bytes);
    ok(&mb(&d, &["regen", "g/assets/koi.glb"]));
    assert_eq!(std::fs::read(&glb).unwrap(), bytes);
    // New processing options on the same raw output, offline; the rest of the recipe stays.
    ok(&mb(&d, &["model", "g", "koi", "--reprocess", "--polys", "600", "--turn", "90"]));
    let sc = sidecar(&glb);
    assert!(sc["result"]["triangles"].as_u64().unwrap() <= 600);
    assert_eq!(sc["recipe"]["turn"], 90.0);
    assert_eq!(sc["recipe"]["glow"], 2.0);
    assert_eq!(sc["seed"], 5);
    // Glow by color, recorded, so a plain regen --reprocess repeats it; --no-glow drops it.
    let e = ok(&mb(&d, &["model", "g", "koi", "--reprocess", "--glow-color", "#f08030,#ffffff", "--glow-tolerance", "0.2"]));
    assert!(e.contains("lit by color"), "{e}");
    let lit = sidecar(&glb)["result"]["glow_share"].as_f64().unwrap();
    assert!(lit > 0.0, "{lit}");
    ok(&mb(&d, &["regen", "g/assets/koi.glb", "--reprocess"]));
    let sc = sidecar(&glb);
    assert_eq!(sc["recipe"]["glow_colors"], serde_json::json!(["#f08030", "#ffffff"]));
    assert_eq!(sc["result"]["glow_share"].as_f64().unwrap(), lit);
    ok(&mb(&d, &["model", "g", "koi", "--reprocess", "--no-glow"]));
    assert!(sidecar(&glb)["recipe"]["glow"].is_null());
    // Clips trimmed and cut, recorded, kept by a plain reprocess; `--clip none` puts them back.
    ok(&mb(&d, &["model", "g", "koi", "--reprocess", "--clip", "Swim=0.25..0.75,Flick=swim@0..0.2"]));
    let clips = |sc: &serde_json::Value| sc["result"]["clips"].as_array().unwrap().iter().map(|c| (c["name"].as_str().unwrap().to_string(), (c["seconds"].as_f64().unwrap() * 100.0).round() / 100.0)).collect::<Vec<_>>();
    let sc = sidecar(&glb);
    assert_eq!(clips(&sc), [("Swim".to_string(), 0.5), ("Flick".to_string(), 0.2)]);
    assert_eq!(sc["recipe"]["clips"], serde_json::json!(["Swim=0.25..0.75", "Flick=swim@0..0.2"]));
    ok(&mb(&d, &["model", "g", "koi", "--reprocess", "--clip", "Swim=0..0.5"]));
    let sc = sidecar(&glb);
    assert_eq!(sc["recipe"]["clips"], serde_json::json!(["Flick=swim@0..0.2", "Swim=0..0.5"]), "an edit replaces the one making the same clip");
    ok(&mb(&d, &["regen", "g/assets/koi.glb", "--reprocess"]));
    assert_eq!(clips(&sidecar(&glb)).len(), 2);
    ok(&mb(&d, &["model", "g", "koi", "--reprocess", "--clip", "none"]));
    assert_eq!(clips(&sidecar(&glb)), [("Swim".to_string(), 1.0)]);
    assert!(!mb(&d, &["model", "g", "koi", "--reprocess", "--clip", "Kick=front_kick@0..1"]).status.success());
    let o = mb(&d, &["model", "presets"]);
    assert!(o.status.success() && String::from_utf8_lossy(&o.stdout).contains("hit_to_body_01"));
    // A name that isn't a Tripo preset is refused before anything is spent.
    let o = mb(&d, &["model", "g", "boss", "a mech", "--animate", "idle,punch", "--dry-run"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("box_01"), "{}", String::from_utf8_lossy(&o.stderr));
    // Variants and picking one.
    ok(&mb(&d, &["model", "g", "crate", "a wooden crate", "--provider", "mock", "--variants", "2", "--polys", "300", "--texture-size", "64"]));
    assert!(g.join("art/variants/crate-2.glb").exists() && g.join("art/variants/crate-sheet.png").exists());
    ok(&mb(&d, &["model", "g", "crate", "--pick", "2"]));
    let sc = sidecar(&g.join("assets/crate.glb"));
    assert_eq!(sc["recipe"]["origin"], "ground");
    // mb art check reads the model and flags the mock.
    let e = ok(&mb(&d, &["art", "check", "g"]));
    assert!(e.contains("assets/koi.glb") && e.contains("tris") && e.contains("mock placeholder"), "{e}");
    // Without a key: a dry run prices the default (Tripo P1, rig and one clip: $0.75); a real run says how to get a key.
    let e = ok(&mb(&d, &["model", "g", "fish", "a koi", "--animate", "swim", "--dry-run"]));
    assert!(e.contains("tripo P1-20260311") && e.contains("$0.7500"), "{e}");
    let o = mb(&d, &["model", "g", "fish", "a koi"]);
    let e = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success() && e.contains("TRIPO_API_KEY") && e.contains("make_glb.py"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_model_made_earlier_is_rigged_and_gets_clips_without_rigging_again() {
    let d = tmp("model-rig");
    let g = game(&d);
    ok(&mb(&d, &["model", "g", "mech", "a squat walker mech", "--provider", "mock", "--polys", "800", "--texture-size", "64", "--seed", "9"]));
    let glb = g.join("assets/mech.glb");
    assert_eq!(sidecar(&glb)["result"]["rigged"], false);
    // Rig it (no prompt: the model made earlier) with one clip, trimmed.
    let e = ok(&mb(&d, &["model", "g", "mech", "--rig", "--animate", "idle", "--clip", "Idle=0..1.5"]));
    assert!(e.contains("rig from its generation task") && e.contains("kept the unrigged model"), "{e}");
    let sc = sidecar(&glb);
    assert_eq!(sc["result"]["rigged"], true);
    assert_eq!(sc["recipe"]["rig"], true);
    assert_eq!(sc["recipe"]["animate"], serde_json::json!(["idle"]));
    let rig = sc["result"]["rig_task"]["id"].as_str().unwrap().to_string();
    assert!(rig.starts_with("mock-rig-"), "{}", sc["result"]);
    assert_eq!(sc["result"]["clips"][0]["name"], "Idle");
    assert!((sc["result"]["clips"][0]["seconds"].as_f64().unwrap() - 1.5).abs() < 0.01);
    assert!(g.join("art/raw/mech-9.unrigged.glb").exists());
    // More clips later: on the recorded rig, no new rig task; a clip it has isn't made again.
    let e = ok(&mb(&d, &["model", "g", "mech", "--animate", "run,idle"]));
    assert!(e.contains(&format!("on its rig, task {rig}")) && e.contains("idle: already a clip"), "{e}");
    let sc = sidecar(&glb);
    let steps: Vec<&str> = sc["result"]["tasks"].as_array().unwrap().iter().map(|t| t["step"].as_str().unwrap()).collect();
    assert_eq!(steps.iter().filter(|s| s.starts_with("rig (")).count(), 1, "{steps:?}");
    assert_eq!(sc["recipe"]["animate"], serde_json::json!(["idle", "run"]));
    let names: Vec<&str> = sc["result"]["clips"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Idle", "Run"]);
    // Reprocessing keeps the rig, the clips and the trim.
    ok(&mb(&d, &["model", "g", "mech", "--reprocess", "--polys", "500"]));
    let sc = sidecar(&glb);
    assert_eq!(sc["result"]["rig_task"]["id"], rig.as_str());
    assert_eq!(sc["result"]["clips"].as_array().unwrap().len(), 2);
    assert!((sc["result"]["clips"][0]["seconds"].as_f64().unwrap() - 1.5).abs() < 0.01);
    // Weight fixes need a biped's named legs; the mock's spine chain has none.
    let o = mb(&d, &["model", "g", "mech", "--reprocess", "--pin", "cape=spine"]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("named legs"));
    // Asking again for nothing new spends nothing.
    let e = ok(&mb(&d, &["model", "g", "mech", "--rig"]));
    assert!(e.contains("already rigged"), "{e}");
    // A dry run on Tripo prices the rig and the clip, and checks the clip's name first.
    ok(&mb(&d, &["model", "g", "bot", "a robot", "--provider", "mock", "--polys", "300", "--texture-size", "64"]));
    let e = ok(&mb(&d, &["model", "g", "bot", "--animate", "slash", "--provider", "tripo", "--dry-run"]));
    assert!(e.contains("rig from an upload of its raw model, clips slash") && e.contains("$0.35"), "{e}");
    assert!(!mb(&d, &["model", "g", "bot", "--animate", "punch", "--provider", "tripo", "--rig-type", "biped", "--dry-run"]).status.success());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn reprocessing_a_model_keeps_its_recipe_and_its_place() {
    let d = tmp("model-reprocess");
    let g = game(&d);
    // A part made into art/pending/ with every processing option, then moved into assets/.
    let args = [
        "model", "g", "blade", "a beam saber hilt", "--provider", "mock", "--kind", "part", "--glow-color", "#ffffff,#202020", "--glow-tolerance", "0.3",
        "--texture-size", "128", "--maps-size", "32", "--metal", "0.5", "--min-roughness", "0.7", "--no-compress", "--polys", "600", "--seed", "3",
        "--out", "art/pending/blade.glb",
    ];
    ok(&mb(&d, &args));
    std::fs::rename(g.join("art/pending/blade.glb"), g.join("assets/blade.glb")).unwrap();
    std::fs::rename(g.join("art/pending/blade.glb.gen.json"), g.join("assets/blade.glb.gen.json")).unwrap();
    // --reprocess with one new option: written where it is now, the rest as recorded.
    let e = ok(&mb(&d, &["model", "g", "blade", "--reprocess", "--polys", "400"]));
    assert!(e.contains("writing assets/blade.glb (made as art/pending/blade.glb)"), "{e}");
    assert!(!g.join("art/pending/blade.glb").exists(), "nothing written to the old place");
    let glb = g.join("assets/blade.glb");
    let sc = sidecar(&glb);
    let r = &sc["recipe"];
    assert_eq!(r["out"], "assets/blade.glb");
    assert_eq!(r["object"], "part");
    assert_eq!(r["origin"], "center", "a part is centered");
    assert_eq!(r["glow_colors"], serde_json::json!(["#ffffff", "#202020"]));
    let near = |v: &serde_json::Value, x: f64| (v.as_f64().unwrap() - x).abs() < 1e-6;
    assert!(near(&r["glow_tolerance"], 0.3) && near(&r["metal"], 0.5) && near(&r["min_roughness"], 0.7), "{r}");
    assert_eq!(r["maps_size"], 32);
    assert_eq!(r["compress"], false);
    assert_eq!(r["polys"], 400);
    assert!(sc["result"]["glow_share"].as_f64().is_some());
    assert_eq!(sc["result"]["compressed"], false);
    assert!(sc["result"]["metal_rough"]["roughness"].as_f64().unwrap() >= 0.69, "{}", sc["result"]);
    let bytes = std::fs::read(&glb).unwrap();
    // mb regen --reprocess repeats it exactly; so does --reprocess with nothing new.
    ok(&mb(&d, &["regen", "g/assets/blade.glb", "--reprocess"]));
    assert_eq!(std::fs::read(&glb).unwrap(), bytes);
    ok(&mb(&d, &["model", "g", "blade", "--reprocess"]));
    assert_eq!(std::fs::read(&glb).unwrap(), bytes);
    // --out names the model to reprocess and where it's written.
    std::fs::create_dir_all(g.join("assets/parts")).unwrap();
    std::fs::rename(g.join("assets/blade.glb"), g.join("assets/parts/blade.glb")).unwrap();
    std::fs::rename(g.join("assets/blade.glb.gen.json"), g.join("assets/parts/blade.glb.gen.json")).unwrap();
    ok(&mb(&d, &["model", "g", "blade", "--reprocess", "--out", "assets/parts/blade.glb", "--compress"]));
    assert!(!g.join("assets/blade.glb").exists());
    let sc = sidecar(&g.join("assets/parts/blade.glb"));
    assert_eq!(sc["recipe"]["out"], "assets/parts/blade.glb");
    assert_eq!(sc["result"]["compressed"], true);
    // A moved model regenerates in place with mb regen too.
    std::fs::rename(g.join("assets/parts/blade.glb"), g.join("assets/blade2.glb")).unwrap();
    std::fs::rename(g.join("assets/parts/blade.glb.gen.json"), g.join("assets/blade2.glb.gen.json")).unwrap();
    let e = ok(&mb(&d, &["regen", "g/assets/blade2.glb", "--reprocess"]));
    assert!(e.contains("writing assets/blade2.glb"), "{e}");
    assert_eq!(sidecar(&g.join("assets/blade2.glb"))["recipe"]["out"], "assets/blade2.glb");
    // --orient turns it by its shape, is recorded, and mb model info reads the result.
    ok(&mb(&d, &["model", "g", "blade", "--reprocess", "--out", "assets/blade2.glb", "--orient", "tip=+y"]));
    let sc = sidecar(&g.join("assets/blade2.glb"));
    assert_eq!(sc["recipe"]["orient"], "tip=+y");
    assert_eq!(sc["result"]["orient"]["spec"], "tip=+y");
    assert_eq!(sc["result"]["facing_check"]["verdict"], "oriented");
    let o = mb(&d, &["model", "info", "g/assets/blade2.glb"]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success() && out.contains("tip (") && out.contains("anchor") && out.contains("long axis"), "{out}");
    assert!(!mb(&d, &["model", "g", "blade", "--reprocess", "--out", "assets/blade2.glb", "--orient", "nose=+z"]).status.success());
    let _ = std::fs::remove_dir_all(&d);
}
