//! Loop metering: guarantees a guest callback can't run forever.
//!
//! On iOS neither releasing a WKWebView nor `Worker.terminate()` stops a wasm
//! loop that never yields (Phase 0/1 measurements), so the client rewrites
//! `game.wasm` before serving it: every `loop` starts by decrementing a fuel
//! counter and traps when it hits zero. The runtime refills the counter
//! (exported as `__mb_fuel`) before each callback. Without loops, wasm can't
//! run unboundedly: recursion hits the stack limit and bulk memory operations
//! are bounded by memory size. Exhaustion is deterministic, so it replays.

use walrus::ir::{BinaryOp, Instr, InstrLocId, InstrSeqId, UnaryOp, Value, Visitor, dfs_in_order};
use walrus::{ConstExpr, GlobalId, Module, ValType};

/// Name of the exported fuel global.
pub const FUEL_EXPORT: &str = "__mb_fuel";
/// Exports with this prefix are reserved for the platform.
pub const RESERVED_PREFIX: &str = "__mb";

/// The version of this pass. Its output is a pure function of the input for
/// one version, so a client may keep metered wasm by [`cache_key`] (SPEC
/// "Studio bundles", "validated and metered once"). Bump it whenever
/// `instrument`'s output changes: the `__mb_fuel` export, its type, where
/// fuel is charged, or walrus's emitting. Entries of other versions are
/// discarded.
pub const METER_VERSION: u32 = 1;

/// What metered wasm may be cached under: [`METER_VERSION`] and the SHA-256
/// of the unmetered bytes, as `<version>-<sha hex>`.
pub fn cache_key(wasm: &[u8]) -> String {
    use sha2::Digest;
    format!("{METER_VERSION}-{}", crate::bundle::hex(&sha2::Sha256::digest(wasm)))
}

/// Metered engines in a directory, one file a key (`<key>.wasm`): the
/// metered wasm, then its SHA-256, so a file that's cut short or changed is
/// metered again, not run. Writing an entry deletes those of other metering
/// versions ([`DiskCache::sweep`]). `mb` keeps them in its cache directory,
/// the app in `Caches/engines` (docs/STUDIO-2.md §4.8).
pub struct DiskCache(pub std::path::PathBuf);

fn sha(b: &[u8]) -> [u8; 32] {
    <sha2::Sha256 as sha2::Digest>::digest(b).into()
}

impl DiskCache {
    /// Deletes the entries of other metering versions (after an update that
    /// changed the pass).
    pub fn sweep(&self) {
        let current = format!("{METER_VERSION}-");
        for e in std::fs::read_dir(&self.0).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".wasm") && !name.starts_with(&current) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

impl crate::bundle::MeterCache for DiskCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        let mut b = std::fs::read(self.0.join(format!("{key}.wasm"))).ok()?;
        let body = b.len().checked_sub(32)?;
        if sha(&b[..body]) != b[body..] {
            return None;
        }
        b.truncate(body);
        Some(b)
    }

    fn put(&self, key: &str, metered: &[u8]) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        if std::fs::create_dir_all(&self.0).is_err() {
            return;
        }
        // Written whole, then renamed: two writers at once (processes, or
        // threads of one) never share a temporary file, and a reader never
        // sees half of one.
        let tmp = self.0.join(format!(".{key}.{}.{}.tmp", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let data = [metered, &sha(metered)].concat();
        if std::fs::write(&tmp, data).is_err() || std::fs::rename(&tmp, self.0.join(format!("{key}.wasm"))).is_err() {
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        self.sweep();
    }
}

/// Returns the metered module.
pub fn instrument(wasm: &[u8]) -> Result<Vec<u8>, String> {
    let mut module = Module::from_buffer(wasm).map_err(|e| format!("metering: {e}"))?;
    let fuel = module.globals.add_local(ValType::I32, true, false, ConstExpr::Value(Value::I32(i32::MAX)));
    module.exports.add(FUEL_EXPORT, fuel);

    for (_, func) in module.funcs.iter_local_mut() {
        let mut loops = Loops::default();
        dfs_in_order(&mut loops, func, func.entry_block());
        let builder = func.builder_mut();
        for seq in loops.0 {
            let trap = builder.dangling_instr_seq(None).unreachable().id();
            let ok = builder.dangling_instr_seq(None).id();
            check(&mut builder.instr_seq(seq), fuel, trap, ok);
        }
    }
    Ok(module.emit_wasm())
}

/// `if (fuel == 0) unreachable; fuel -= 1;` at the start of a loop body.
fn check(seq: &mut walrus::InstrSeqBuilder<'_>, fuel: GlobalId, trap: InstrSeqId, ok: InstrSeqId) {
    let prologue: [Instr; 7] = [
        walrus::ir::GlobalGet { global: fuel }.into(),
        walrus::ir::Unop { op: UnaryOp::I32Eqz }.into(),
        walrus::ir::IfElse { consequent: trap, alternative: ok }.into(),
        walrus::ir::GlobalGet { global: fuel }.into(),
        walrus::ir::Const { value: Value::I32(1) }.into(),
        walrus::ir::Binop { op: BinaryOp::I32Sub }.into(),
        walrus::ir::GlobalSet { global: fuel }.into(),
    ];
    for (i, instr) in prologue.into_iter().enumerate() {
        seq.instr_at(i, instr);
    }
}

#[derive(Default)]
struct Loops(Vec<InstrSeqId>);

impl<'i> Visitor<'i> for Loops {
    fn visit_instr(&mut self, instr: &'i Instr, _: &'i InstrLocId) {
        if let Instr::Loop(l) = instr {
            self.0.push(l.seq);
        }
    }
}
