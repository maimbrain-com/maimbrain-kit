//! Studio bundles (SPEC "Studio bundles", docs/STUDIO.md): a game whose
//! `game.wasm` is the published Studio engine and whose project ships under
//! `assets/`, named by the manifest's `engine = "studio@<major>.<minor>"`.
//!
//! What's checked here, on top of every other bundle rule:
//! - `engine` is `studio@MAJOR.MINOR`;
//! - the project is there: `assets/studio.toml`, `assets/game/main.lua` and
//!   the generated `assets/files.txt`, every file it lists, and nothing it
//!   lists outside `assets/`;
//! - engine 2.0 and later (docs/STUDIO-2.md §4.13): every `.lua` under
//!   `assets/game/` has its Luau bytecode beside it (`main.lua` →
//!   `main.luac`), of a version the engine's VM loads ([`BYTECODE_VERSIONS`]);
//! - `game.wasm` is the engine build for the manifest's import set (with
//!   tilt or without, or the 3D build, [`variant`]; engine 2.0 has a 2D
//!   and a 3D build for each sensor set);
//! - whether `game.wasm` is a published engine build ([`PUBLISHED`]). A
//!   local build isn't, so `mb build` says so as a warning; the server
//!   refuses a Studio upload that isn't (`Report::studio.published`).

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::manifest::{Manifest, Sensor};

/// Bundle formats this validator opens beyond the plain one. An app sends
/// them with its feed requests, so the server only offers games it can play
/// (an older app's validator refuses a Studio bundle: `engine` is a field it
/// doesn't know, and a bundle over 10 MB, `large`, [`crate::Report::formats`]).
/// `studio2` is engine 2.0 ([`Manifest::needs_formats`]) and `mb2d3` a game
/// declaring `mb2d = 3`, as every 2.0 game does: apps claim them from their
/// engine 2.0 release on (docs/STUDIO-2.md §9.2, 1e). `mb3d3` is a game
/// declaring `mb3d = 3`, as engine 2.0's 3D build does from Phase 2a
/// (§9.3): apps claim it from the release after that. `mb3d4` is a game
/// declaring `mb3d = 4` (ground surfaces, Decided 70), as engine 2.0's 3D
/// build does from then: apps claim it from the release that brings it.
/// `mb2d4` and `mb3d5` are games with shaders (sprite shaders, shader
/// materials; docs/STUDIO-2.md §4.15): only engine 2.0's shader builds
/// ([`Variant::PlainShaders`], [`Variant::ThreeDShaders`]) declare them.
pub const FORMATS: &[&str] = &["studio", "studio2", "mb2d3", "mb3d3", "mb3d4", "mb2d4", "mb3d5", "large"];

/// The Luau bytecode versions engine 2.0's VM loads (the `luau` crate's
/// `BYTECODE_VERSION_MIN..=MAX`; mb-cli's tests keep them equal). A chunk's
/// first byte is its version.
pub const BYTECODE_VERSIONS: std::ops::RangeInclusive<u8> = 3..=14;

/// The engine major that runs Luau, ships Lua as bytecode and is format `studio2`.
pub const LUAU_MAJOR: u32 = 2;

/// The generated list of the project's files (paths under `assets/`).
pub const INDEX: &str = "assets/files.txt";

/// The engine's builds: one per import set (SPEC "Studio bundles").
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    /// The Arcade, no sensors.
    Plain,
    /// The Arcade with `mb_tilt` (`player.tilt`): the manifest declares `sensors = ["tilt"]`.
    Tilt,
    /// 3D games (`kind = "3d"`): mb3d 2 and
    /// haptics, so the manifest declares `stdlib = { mb3d = 2, mb2d = 1 }`,
    /// `sensors = ["haptics"]` and `perf_tier = "full"`.
    #[serde(rename = "3d")]
    ThreeD,
    /// Engine 2.0's sensor sets (docs/STUDIO-2.md §4.5), each with the one
    /// before it: `motion` (tilt, motion and attitude: CoreMotion, which asks
    /// no permission), then `mic` (those and loudness), in 2D and in 3D.
    Motion,
    Mic,
    #[serde(rename = "3d-motion")]
    ThreeDMotion,
    #[serde(rename = "3d-mic")]
    ThreeDMic,
    /// Engine 2.0's 2D build with sprite shaders (a project with
    /// `shaders/*.wgsl`, docs/STUDIO-2.md §4.15): it imports mb2d 4, so the
    /// manifest declares `mb2d = 4`. No sensors beyond haptics.
    #[serde(rename = "plain-shaders")]
    PlainShaders,
    /// Engine 2.0's 3D build with shader materials and sprite shaders: it
    /// imports mb3d 5 and mb2d 4. Games without shaders keep the builds
    /// above, so they never need the app that opens these.
    #[serde(rename = "3d-shaders")]
    ThreeDShaders,
}

impl Variant {
    /// Engine 1.0's builds.
    pub const ALL: [Variant; 3] = [Variant::Plain, Variant::Tilt, Variant::ThreeD];
    /// Engine 2.0's: a 2D and a 3D build for each sensor set, and the shader builds.
    pub const V2: [Variant; 8] =
        [Variant::Plain, Variant::Motion, Variant::Mic, Variant::ThreeD, Variant::ThreeDMotion, Variant::ThreeDMic, Variant::PlainShaders, Variant::ThreeDShaders];

    pub fn as_str(self) -> &'static str {
        match self {
            Variant::Plain => "plain",
            Variant::Tilt => "tilt",
            Variant::ThreeD => "3d",
            Variant::Motion => "motion",
            Variant::Mic => "mic",
            Variant::ThreeDMotion => "3d-motion",
            Variant::ThreeDMic => "3d-mic",
            Variant::PlainShaders => "plain-shaders",
            Variant::ThreeDShaders => "3d-shaders",
        }
    }

    /// A 3D build (mb3d, the full tier).
    pub fn is_3d(self) -> bool {
        matches!(self, Variant::ThreeD | Variant::ThreeDMotion | Variant::ThreeDMic | Variant::ThreeDShaders)
    }

    /// The builds with shaders (mb2d 4, and in 3D mb3d 5).
    pub fn shaders(self) -> bool {
        matches!(self, Variant::PlainShaders | Variant::ThreeDShaders)
    }

    /// The sensors this build imports, beyond haptics.
    pub fn sensors(self) -> &'static [Sensor] {
        match self {
            Variant::Plain | Variant::ThreeD | Variant::PlainShaders | Variant::ThreeDShaders => &[],
            Variant::Tilt => &[Sensor::Tilt],
            Variant::Motion | Variant::ThreeDMotion => &[Sensor::Tilt, Sensor::Motion, Sensor::Attitude],
            Variant::Mic | Variant::ThreeDMic => &[Sensor::Tilt, Sensor::Motion, Sensor::Attitude, Sensor::Loudness],
        }
    }

    /// Engine 2.0's build for a kind (`three`) and the sensors a game reads:
    /// the smallest set that has them all (a tilt game's is `motion`).
    pub fn v2(three: bool, reads: &[Sensor]) -> Variant {
        let has = |s: Sensor| reads.contains(&s);
        let mic = has(Sensor::Loudness);
        let motion = has(Sensor::Tilt) || has(Sensor::Motion) || has(Sensor::Attitude);
        match (three, mic, motion) {
            (false, true, _) => Variant::Mic,
            (false, false, true) => Variant::Motion,
            (false, false, false) => Variant::Plain,
            (true, true, _) => Variant::ThreeDMic,
            (true, false, true) => Variant::ThreeDMotion,
            (true, false, false) => Variant::ThreeD,
        }
    }
}

/// A published engine build: the CDN serves it, and the server accepts
/// Studio bundles carrying exactly these bytes.
#[derive(Clone, Copy, Debug)]
pub struct Published {
    /// "1.0".
    pub version: &'static str,
    pub variant: Variant,
    /// SHA-256 of its `game.wasm`, hex.
    pub sha256: &'static str,
}

/// Every published engine build: `just studio-engine` makes them
/// (games/studio-engine/published/, which `mb` embeds and packs with) and
/// prints these lines. A build stays listed after it's replaced, so bundles
/// packed with it stay accepted.
pub const PUBLISHED: &[Published] = &[
    Published { version: "1.0", variant: Variant::Plain, sha256: "867329d1dea0dd04726bcfb906389f47ba9030dadb6285de8e04d81c15ef7ade" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "1b2d6711048ca91e37c5deccfb4f2952d019feef21c9d824604ad12cc8ad143c" },
    // render reads the round's score, lives and level from update's copy, not the VM.
    Published { version: "1.0", variant: Variant::Plain, sha256: "8575418f35be1bb6031def1dc1a84823d1d7f7f90260432dcf06634b03b5ab20" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "cae3f50d0b1491b7cb8e2f9dbfecc90a222cd32d40981a5977628d83c13144e1" },
    // The sandbox keeps the saved-data load(key).
    Published { version: "1.0", variant: Variant::Plain, sha256: "9345f2d32612a0c009fc7e14d2da6ad58fae38a4d6a0b077e5e200c3683c2267" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "b526a56348076b965ee5f6db5000d4bf5d36adc177b0b4c99f7bc7cb2776ef6f" },
    // What start() asks for (music, popups, sounds) plays on the first frame.
    Published { version: "1.0", variant: Variant::Plain, sha256: "cf696fa468c4ce91cdfc688413a892b9614685c17daa2dce323f915e0c92d0ce" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "70e6729236b73ae82fd32e0eb5583926128e473ef4707e3027bd4cff7059b110" },
    // mb play's frames get the game's text() and popups.
    Published { version: "1.0", variant: Variant::Plain, sha256: "5712bc7fae53214ac9a55f53e822bd7bca6591ece01dfd18f43ed40edd0f865b" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "f24c4c7211732e374ad962b1e55b6c8a910b1eb58220758ea248d98d5e26f10f" },
    // text() takes align, width (wrapping), scale and shadow; text_width; text keeps the canvas's size on any screen.
    Published { version: "1.0", variant: Variant::Plain, sha256: "1e52074739b200c4ad711e7cc042e30263f83e2920eeceaef9cd096c12d312c6" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "a8a120a26a75715ffef045fcea54a86e8b45c7fdad16b2b2d932e4f6a188820e" },
    // A scenario's set globals land before start(), and stay for the rounds after.
    Published { version: "1.0", variant: Variant::Plain, sha256: "a7d0bed83c174307832c0da6c38d48c454bc0b9bc5de05ee24354a28d28128ef" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "1e9d7491842f56d6fc524fc7c4ac1d622296c9e2f4c7118a0873b090a1363045" },
    // The 3D build (3D games); every error gets its line.
    Published { version: "1.0", variant: Variant::Plain, sha256: "63f45b2693f56fda82ebb5fe9ba2107354fd2f216dab324b6394374b7b4a76ae" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "0babbec081a826a7384dc913377a92c835d5df9f37efdffc750fc6e7ddbf8f51" },
    Published { version: "1.0", variant: Variant::ThreeD, sha256: "d8e868acee022c362c6fcfa5e7d53128e83bcb69042ba51c3f3ca4b2f6ec4caf" },
    // studio.toml says kind = "2d" or "3d" (rung and view, the old way, still read).
    Published { version: "1.0", variant: Variant::Plain, sha256: "688e31535ab28820c02050242f14f31506b616ee289f1eae4b8bf9a60c188321" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "8c10c4fe27895564a015d6ca8178f0695ee8dc90a9d9cbecdbe9febb53746eae" },
    Published { version: "1.0", variant: Variant::ThreeD, sha256: "7df4d535d7c00f78adaa9b652db74ad4c8907e530f6774bf7dd3194ec3cbdb91" },
    // A tap before the project has loaded is ignored (no "the project hasn't loaded yet" over the title).
    Published { version: "1.0", variant: Variant::Plain, sha256: "ea0b704f9a822e4e768733c3693d9f1cefc572f5ea1f20b516685cbe8908dd55" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "04e0e17b73ae5387f08c4080b959d4bb398fbaec06b2cd8c15b81a4a6d2b411d" },
    Published { version: "1.0", variant: Variant::ThreeD, sha256: "9cf15ae14bf63c12b392f966fd16dbf3b0cab3ff632194f0b8d9435c451313d4" },
    // A long game-over heading shrinks to fit the card, or wraps to two lines (the kit's results card).
    Published { version: "1.0", variant: Variant::Plain, sha256: "5fa8ad15a92fd0d488bf573839ed38e04ceec677f2054d02b4504df273b42d09" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "b304f7ee6fda46c21c76516e950f7fe9dfe9760d45197f047b7ce4c4e0374cd3" },
    Published { version: "1.0", variant: Variant::ThreeD, sha256: "d88ebc7ed5686ab13c90fca93e1e194f9330d9d378e004dd1971dfcbd764d2bc" },
    // The debug menu: a scenario or level jump during a round starts it again, Restart round,
    // a scenario replaces the last one, debug.invincible().
    Published { version: "1.0", variant: Variant::Plain, sha256: "aaeeaca60c4dac8a4fa36cf5d2a620d453d90328c0ea462575c3bb059f08d2dc" },
    Published { version: "1.0", variant: Variant::Tilt, sha256: "85f425c6a1466277d6af3519f09628bb4d8b17e4287256d12e39922c902ffd82" },
    Published { version: "1.0", variant: Variant::ThreeD, sha256: "c19a7f108a3b525871192208f1ceccb3afe5b7fda913042ffa1391f79709cd87" },
    // Engine 2.0 (games/studio-engine2, `just studio-engine2`): Luau, LÖVE's callbacks, the retained scene.
    // Until 2.0 ships to players (Phase 1), a new build replaces its line rather than adding one.
    Published { version: "2.0", variant: Variant::Plain, sha256: "6c0df757256013ab920b2a9ac08f089b0fd482a6a00d969230fa8e2a1310c31b" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "fc41cf2886a63b9af296122c132ec68b58a98de51bb615c7df294cdd6c900e6e" },
    // Phase 2a (docs/STUDIO-2.md §9.3): the 3D API. Phase 1's builds stay, as games are made on them;
    // the 3D build now imports mb3d 3's batched transforms, so its games declare mb3d = 3.
    Published { version: "2.0", variant: Variant::Plain, sha256: "6ec503a8183e2820e3d1b8e96ed173bff61c5cfeaa3590a4d150115d6118d0cb" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "1138326f5db49f5d1b665713fa05089cda9c542e415e9bd97cedc3a5227ba50f" },
    // Phase 2e: round.subject and the readability check's frame variants (mb play only). 2c's builds stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "c666e728ab3ea47dbcba546fb72f0a82740dd9b504a1df8b173e420868ba3bf9" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "65beaae423433982609f09a099caf9870783bba8215aeb6188647590e70973f7" },
    // Phase 3a: love.physics in the 2D build, Godot's bodies, joints, vehicles and the terrain's ground in
    // the 3D build. No new imports; the builds before stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "b4baa5e1498694d6442959ce9d0ce06b2e579cffc1ba4c71ca4b6fa1b57ea637" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "6d82ed06f341d384c1c7a3b2449a52995686b3c3fdef43fa795118f4ac647fb0" },
    // Phase 4c (docs/STUDIO-2.md §9.5): round.post and round.board, the daily board's DAILY and SCORES.
    Published { version: "2.0", variant: Variant::Plain, sha256: "1dc276d919722090ea56ea0a381ca8f771b231ac73eccd2b223a3879a32786ee" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "13872c37ae9a88de3a5e5479cefa03fd89c31fa51a9fe6cb7fb66f126396e00e" },
    // Phase 4b (docs/STUDIO-2.md §9.5): the UI kit (widgets, hints, lessons, the stats card, the combo)
    // and the bot that follows hints.
    Published { version: "2.0", variant: Variant::Plain, sha256: "d9758aa1e57a318a6ed2dcfb8d3081d5f17d8bdd4647196d182f17d1b3bbc502" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "1f679efba2404aae232c464b21f256065346a787dc72027d9826d616942a05cd" },
    // Ground surfaces (docs/STUDIO-2.md Decided 70): the 3D build draws a Terrain with mb3d 4's
    // ground shader, so its games declare mb3d = 4 and need format mb3d4. Earlier builds stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "95f918cbdcd7cfb1e31770f17750547110c95282b75170e0fd66ab2658c00b6a" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "f4fa88109f3ee1efebf0c53dd7ad16d2ecb7ef962d44c8edd5766903481b5fd5" },
    // Phase 3c: spring, fx.punch, fx.slowmo, audio buses (the existing mb_bus_fx) and foley. No new
    // imports; the builds before stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "e3426d97d9a420e604495f4640fb7fae751ad1a6f0b3e4685696c8539c1b8d9a" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "8ff59ad84136aaa90deca06fd356c9e1c081847ee2f27a7bc7e508a7b8fe5d09" },
    // Phase 3d: music graphs, setlists, mood, zones and on("beat") on the round's clock. Earlier builds stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "e411002e4d5ea7d130794adc82f7ee63bcb07f765fa1667836e1194a433237e3" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "9d639d0e127fab95889b7e827edfc6cad16ecc6622b106bc49a38585fb2530b7" },
    // Phase 3e: the sensor builds (motion, mic; 2D and 3D), sensors.*, debug.mark and the smoothness check's poses.
    Published { version: "2.0", variant: Variant::Plain, sha256: "eb67d209a623b3ee665c20354193ceb85bed53787f0ad1329fef42c0901620c1" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "56f6ae1bb691c1bce1dfdae9f54459deffb604b21e87bfbe205cc8c2ae1020b8" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "802d5f7ea1a23c1cfec3c89880ca9c2f02123af40cd0500af0379f7820e189cd" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "4d7fe34506e07677836e2223982f3ef87f55351c847e805ace99af65b74454d3" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "132e85ab22911bd583912af3574479d0b19aae159762a6f7d6f04c0998a5aeea" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "bfd4413f1d1b85949f4cb02a7aaaa3720e9117400da46608724a30bf987d1b16" },
    // Phase 4a (§9.5): cast, say, dialogue{} with choices, bark, narrate and cutscene{} on the SDK's
    // Dialogue and Sequence, in all six builds. No new imports; the builds before stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "ad48ffdb5d067c6c6c7e741f4e739f990cd7223bbc4a7dced9745e124c909088" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "18ab4110d068f43be40cdee41149b4aedc5c3e904b4d6fcd3f5d22fa5dc90e10" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "59b674296c02b1fb750bdfa96f136988694ae6316df59ea942dab162176865e0" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "1c05fe928c7b1c2a012aa360b43c77c2a9b101e60e612a9dcb81ff092f0e64e5" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "d8666bbe74007d6c3bfe2ed270bd31c108b58690cc7e5701b9301af62696d75b" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "f663d7e656e309b7106e3012f9d8a664e010ad1dd4fdc2c3fbc961e136805211" },
    // Phase 4d (docs/STUDIO-2.md §9.5): `act.preset` for a bot.lua (the trivia starter's reader), in
    // all six builds. No new imports; the builds before stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "e1a679fdada0881952740a945c7d43e4daa4774d7ad5dd6741587de998ea23a7" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "77b6498524ddb62ad3f6b2ceb83121ab40fdc3116a3ff1ef8834d8c342dbabe5" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "ca3fc25714eb1b449459b2a2eca31509e8a0178f788e0d6d2bfe7abd11c078cd" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "248f881cfebc152add6c6a1d3121b832a2e93fb112527289304c5f0bcfd7bc8c" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "84163be608d7f2d2d1a888de57caad422e71a1066d0fe40326a5c516700aa95c" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "23a3dc4c3933c03a5cd1167124f89239f1045bca292f00ed82c0808724d59edf" },
    // Phase 3b: CharacterBody2D and the quick path's platformer on it, Tilemap:path in the 2D build;
    // CharacterBody3D, NavigationRegion3D and NavigationAgent3D (Recast) in the 3D builds. No new imports.
    Published { version: "2.0", variant: Variant::Plain, sha256: "b1b3d635c966ed398d9d09d2b7fd3ebe24e4d7fe52a33efd3399eb25cd519125" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "80e61ab4fccb05871bc56fa15afbf31e9d13833312bbc768b359195ff8c38349" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "fe9666b166a64767717dc6df56e2b667a0d83f52d1b37cb6a80404ee09bb486d" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "99c499ac8a691cb227976357a5b296135303444286faea486c3f62d17fcb4dea" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "1cd4c904dabeff6197d67424fa64121074bebd500ee124ef6477c600bf5f255f" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "70ee59656a028e3f920fa76c6a74a97f60a8734ada7dd10f9bfb3a70971bd528" },
    // Phase 5 (docs/STUDIO-2.md §9.6): shader materials and sprite shaders. The builds without shaders
    // import nothing new; projects with shaders/*.wgsl get the shader builds (mb2d 4, mb3d 5; no sensors),
    // so only their games need the app that opens them. Earlier builds stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "30bac1186906c3b2ecaf8aacf4791176284ea74e94f0f28d8d3471eccb310ca9" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "2fb687bd7bbb93c5c3f0bb44cb1842a8c277805b235e0ab7913a37f039a1317a" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "b68cf9aeeb0783a0033b14e76bbeda4daaec2513dead82b1703ae77b4151f3e6" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "0eaf5608695be751a17587921944bf0c8a4e0f8f7d3c827985f0d0b161199d95" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "6c91737609280d3b484e9b48682654d2c01589a4f600c7a41a9368f312b6767b" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "31969c03d9cf1f9b94bb8ed04558992313ebb5d382a932ddbc0c2613e871e4d1" },
    Published { version: "2.0", variant: Variant::PlainShaders, sha256: "3afa908620078be1069b33fd659c7ddbe332eef9f2dfa01198b7a5accef9f64f" },
    Published { version: "2.0", variant: Variant::ThreeDShaders, sha256: "92a502ce2d605d80802ad2651cd89805aa610557cfd3b78b6fffc67e5f468109" },
    // The polish after the phases: the engine's Rust at opt-level "s" (Luau's C++ at -O3), 13% smaller;
    // round.endDelay, fx.flash's alpha, ui.button's tone, the card buttons on a panel, the bot's steering.
    // No new imports; the builds before stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "8659e54d5e163ff13d40cee4ef9b0e97eaca537305aa36cc8281390a8a4cf07a" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "dac9e0f1ed291a6419c723ff502d0b58d6d1564cf4c7d32766939ad8a68299f0" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "3c76c35d8c2577c3f75622ad795047af7dd71815276ab9ace56c6ee371be1a48" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "507b4be8b51bb6f24e9d6621b48756743044bd044bf07e7d766855196e99d06f" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "7e0e628f78403550c9eca370c243a3d2adaa42da9df4307d34d803a432c01493" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "b1aafe05c986d4915964541ce266e979734c2afd28394c771db99cce35b00104" },
    Published { version: "2.0", variant: Variant::PlainShaders, sha256: "137da3b94162b9474c29c8aad916be1933dac3b95e88db761dd70a4672509c5b" },
    Published { version: "2.0", variant: Variant::ThreeDShaders, sha256: "ae4131b78cafe0cb8c848dd489b2e3119d7565471b3e7fe3a4e5be860a62e5eb" },
    // Phase 3 and 4's acceptance: love.physics' World:update takes LÖVE's velocity iterations.
    Published { version: "2.0", variant: Variant::Plain, sha256: "e2fb4f0eaf471923cf7c9b27550dfd1965e129e5021ca0ce016c3e2fe7ce6b4a" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "2213ee74a895a070350b5927b830810f86c61bd249217cdad78aee83c15c95d4" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "7c6129c20f075b12c46f2e64c12d8a0d855521afd54ddf0573575fb19e59ef5d" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "abfdfba49150fc4a7fb6989519b2db74ac9539be1ddcaf2d6860ad273c0e478a" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "20d1ccc55e5d489a2276a629af9b7e3799092a8d2ae8adf8cd3b644477c104f4" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "87da52c6067da72e3221a1a4a2d8f3c9e20b50c1dbd5b7685528950a64e9fab9" },
    Published { version: "2.0", variant: Variant::PlainShaders, sha256: "55e92acaa0d24e9d17d02f544d751c797fbe5c4c317a5ecc2ca83cf4fd6e878a" },
    Published { version: "2.0", variant: Variant::ThreeDShaders, sha256: "002197ccfd391c971d6320c42c6ebf4ff7ff39893525775b24d6dfaee208eaaf" },
    Published { version: "2.0", variant: Variant::Plain, sha256: "a7dccd28159566a60fb4fdcdd60695e8697b5ff68d2dcf8ce836e1940136711b" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "e71d9e39ecaca4759ec7c5a411ea289e0c7350b597b0609a92edd8ab234f42b3" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "6fc6f833511bd44cb26d2b47c595c9d4e2250a45413e6875f68b5bcdc031df28" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "83a5ed25403f1f8b8c6a086f799c9a043f2b8201c5c479710fdd8b8bdb79f3f1" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "017bf023e5714083d439f0e56cfa1037d6997a32719f07cb80369625f25a756c" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "e0e1e6458db669b54983ef86614fbd90552c9d03cea151574933040e7d27bb05" },
    Published { version: "2.0", variant: Variant::PlainShaders, sha256: "8b5c85393e987a136091de7273c3688268d03b6843ca0c7b48a69985dd0ff9c8" },
    Published { version: "2.0", variant: Variant::ThreeDShaders, sha256: "796aff4649d9622a7315dc98b28bcf3ea9908688efbe3f870a13643cb8fe0406" },
    // Kit 0.10.0: built with SDK 0.10.0 (its maimbrain-sdk section says so). No other change; the builds before stay.
    Published { version: "2.0", variant: Variant::Plain, sha256: "8256d5bf5a9c849211a96ddb59501de200ae2f94d26cd3aa10159c7d57ce74d7" },
    Published { version: "2.0", variant: Variant::Motion, sha256: "3bd7e8804edaed6c5ad48572912aec6833ea105988259fd80aeda0e021009cd1" },
    Published { version: "2.0", variant: Variant::Mic, sha256: "a01434c88e703b0e4b6f067e171a2fac002535e3720cc7039ed7c39e948df661" },
    Published { version: "2.0", variant: Variant::ThreeD, sha256: "3d9d100dcd020a0b26f4c9c7d3122d2c99743690e3ef12925328cc3f7078324b" },
    Published { version: "2.0", variant: Variant::ThreeDMotion, sha256: "2193c62538b7449f59f7e95619bed49f57669a235df29344fac83771a9bb09c4" },
    Published { version: "2.0", variant: Variant::ThreeDMic, sha256: "78f49958c6a049c2910579697c8a8cb700aefe59c3b370d48086c9042be83a4e" },
    Published { version: "2.0", variant: Variant::PlainShaders, sha256: "a0785f8f97b71e5d7f4c386f5c66211f9bad79481a7539a255960ce11c446519" },
    Published { version: "2.0", variant: Variant::ThreeDShaders, sha256: "0a860e1c7d1a310759edfc64878b72eff9b282d7620bd52f49730c06c1d3ed61" },
];

/// What the report says about a Studio bundle.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Info {
    /// "1.0".
    pub engine: String,
    pub variant: Variant,
    /// `game.wasm` is a published engine build of that version and variant.
    pub published: bool,
}

/// `studio@1.0` → (1, 0).
pub fn parse_engine(s: &str) -> Option<(u32, u32)> {
    let v = s.strip_prefix("studio@")?;
    let (a, b) = v.split_once('.')?;
    let num = |x: &str| (!x.is_empty() && x.len() <= 4 && x.bytes().all(|c| c.is_ascii_digit())).then(|| x.parse().ok()).flatten();
    Some((num(a)?, num(b)?))
}

/// The engine build a manifest needs: only the 3D build imports mb3d.
/// Engine 2.0's also follow the sensors it declares ([`Variant::v2`]).
/// The engine build a manifest needs: only the 3D builds import mb3d, and
/// only the shader builds mb3d 5 or mb2d 4.
pub fn variant(m: &Manifest) -> Variant {
    // Shaders (mb3d 5, mb2d 4; docs/STUDIO-2.md §4.15): the shader builds.
    if m.stdlib_version("mb3d") >= 5 {
        return Variant::ThreeDShaders;
    }
    if !m.stdlib.contains_key("mb3d") && m.stdlib_version("mb2d") >= 4 {
        return Variant::PlainShaders;
    }
    if m.engine.as_deref().and_then(major).is_some_and(|v| v >= LUAU_MAJOR) {
        return Variant::v2(m.stdlib.contains_key("mb3d"), &m.sensors);
    }
    if m.stdlib.contains_key("mb3d") {
        Variant::ThreeD
    } else if m.sensors.contains(&Sensor::Tilt) {
        Variant::Tilt
    } else {
        Variant::Plain
    }
}

/// The engine's major version, for a manifest's `engine` (`studio@2.0` → 2).
pub fn major(engine: &str) -> Option<u32> {
    parse_engine(engine).map(|(m, _)| m)
}

/// Whether `wasm` is the published build of `version` for `variant`, in `table`.
pub fn is_published(table: &[Published], version: &str, variant: Variant, wasm: &[u8]) -> bool {
    let sha = crate::bundle::hex(&Sha256::digest(wasm));
    table.iter().any(|p| p.version == version && p.variant == variant && p.sha256 == sha)
}

/// Checks a Studio bundle's files (the manifest names an engine).
pub fn check(m: &Manifest, files: &BTreeMap<String, Vec<u8>>, errors: &mut Vec<String>, warnings: &mut Vec<String>) -> Option<Info> {
    let engine = m.engine.as_deref()?;
    let Some((major, minor)) = parse_engine(engine) else {
        errors.push(format!("manifest: engine = {engine:?} isn't a Studio engine (\"studio@1.0\")"));
        return None;
    };
    for required in ["assets/studio.toml", "assets/game/main.lua", "assets/files.txt"] {
        if !files.contains_key(required) {
            errors.push(format!("studio: {required} is missing (build Studio projects with `mb build`)"));
        }
    }
    if let Some(list) = files.get("assets/files.txt") {
        let list = String::from_utf8_lossy(list);
        for line in list.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            if line.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
                errors.push(format!("studio: assets/files.txt lists {line:?}, which isn't a project path"));
            } else if !files.contains_key(&format!("assets/{line}")) {
                errors.push(format!("studio: assets/files.txt lists {line}, but assets/{line} isn't in the bundle"));
            }
        }
    }
    if major >= LUAU_MAJOR {
        check_bytecode(files, errors);
    }
    let variant = variant(m);
    let version = format!("{major}.{minor}");
    let published = files.get("game.wasm").is_some_and(|w| is_published(PUBLISHED, &version, variant, w));
    if !published {
        warnings.push(format!(
            "game.wasm isn't a published Studio engine {version} ({} build): fine for previews and tests, but maimbrain.com accepts only published engines",
            variant.as_str()
        ));
    }
    Some(Info { engine: version, variant, published })
}

/// Engine 2.0's Lua: each chunk's bytecode beside it, of a version its VM
/// loads. (The server compiles the source again before it publishes and
/// compares, SPEC §6; the VM doesn't check bytecode, docs/STUDIO-2.md §4.13.)
fn check_bytecode(files: &BTreeMap<String, Vec<u8>>, errors: &mut Vec<String>) {
    for (path, _) in files.iter().filter(|(p, _)| p.starts_with("assets/game/") && p.ends_with(".lua")) {
        let bc = format!("{path}c");
        match files.get(&bc).and_then(|b| b.first()) {
            None => errors.push(format!("studio: {path} has no bytecode beside it ({bc}): build Studio 2.0 projects with `mb build`")),
            Some(0) => errors.push(format!("studio: {bc} is a compile error, not bytecode")),
            Some(v) if !BYTECODE_VERSIONS.contains(v) => {
                errors.push(format!("studio: {bc} is Luau bytecode version {v}; this engine loads {}–{}", BYTECODE_VERSIONS.start(), BYTECODE_VERSIONS.end()))
            }
            Some(_) => {}
        }
    }
    for path in files.keys().filter(|p| p.starts_with("assets/") && p.ends_with(".luac")) {
        if !files.contains_key(path.trim_end_matches('c')) {
            errors.push(format!("studio: {path} has no source beside it ({}); bundles carry the Lua they ran", path.trim_end_matches('c')));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_names() {
        assert_eq!(parse_engine("studio@1.0"), Some((1, 0)));
        assert_eq!(parse_engine("studio@12.3"), Some((12, 3)));
        for bad in ["studio@1", "studio@1.0.0", "studio@", "lua@1.0", "studio@1.x", "studio@.1"] {
            assert_eq!(parse_engine(bad), None, "{bad}");
        }
    }

    #[test]
    fn published_builds_are_matched_by_sha_version_and_variant() {
        let wasm = b"\0asm engine";
        let sha = crate::bundle::hex(&Sha256::digest(wasm));
        let sha: &'static str = Box::leak(sha.into_boxed_str());
        let table = [Published { version: "1.0", variant: Variant::Plain, sha256: sha }];
        assert!(is_published(&table, "1.0", Variant::Plain, wasm));
        assert!(!is_published(&table, "1.1", Variant::Plain, wasm));
        assert!(!is_published(&table, "1.0", Variant::Tilt, wasm));
        assert!(!is_published(&table, "1.0", Variant::Plain, b"\0asm other"));
    }

    /// Engine 2.0's build: the kind's, with the smallest sensor set that has
    /// what the game reads (each set has the one before it).
    #[test]
    fn engine2_builds_follow_the_sensors() {
        use Sensor::*;
        assert_eq!(Variant::v2(false, &[Haptics]), Variant::Plain);
        assert_eq!(Variant::v2(false, &[Haptics, Tilt]), Variant::Motion, "tilt comes with motion and attitude");
        assert_eq!(Variant::v2(false, &[Attitude]), Variant::Motion);
        assert_eq!(Variant::v2(false, &[Motion]), Variant::Motion);
        assert_eq!(Variant::v2(false, &[Loudness]), Variant::Mic);
        assert_eq!(Variant::v2(true, &[]), Variant::ThreeD);
        assert_eq!(Variant::v2(true, &[Tilt, Attitude]), Variant::ThreeDMotion);
        assert_eq!(Variant::v2(true, &[Loudness, Tilt]), Variant::ThreeDMic);
        assert_eq!(Variant::v2(true, &[Tilt]), Variant::ThreeDMotion);
        for v in Variant::V2.into_iter().filter(|v| !v.shaders()) {
            assert_eq!(Variant::v2(v.is_3d(), v.sensors()), v, "{v:?} is the build for its own sensors");
        }
        // Engine 1.0's manifests pick as they always did.
        let mut m = crate::Manifest::parse("abi = 0\nid = \"dev.x.t\"\nname = \"T\"\nversion = \"0.1.0\"\ncreator = \"@x\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nsensors = [\"tilt\"]\nengine = \"studio@1.0\"\n").unwrap();
        assert_eq!(variant(&m), Variant::Tilt);
        m.engine = Some("studio@2.0".into());
        m.sensors = vec![Haptics, Tilt, Motion, Attitude];
        assert_eq!(variant(&m), Variant::Motion);
    }
}
