#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

RUST_VERSION="${RISTUX_RUST_VERSION:-1.96.0}"
PROBE_DIR="${RISTUX_RUST_BOOTSTRAP_STAGE2_DIR:-${RISTUX_RUST_TARGET_PROBE_DIR:-/tmp/ristux-rust-bootstrap-stage2}}"
LOG="${RISTUX_RUST_BOOTSTRAP_STAGE2_LOG:-$PROBE_DIR/bootstrap-stage2-build.log}"
RUSTC_LOG="${RISTUX_RUST_BOOTSTRAP_STAGE2_RUSTC_LOG:-$PROBE_DIR/bootstrap-stage2-rustc-build.log}"
STAGE1_CODEGEN_LOG="${RISTUX_RUST_BOOTSTRAP_STAGE2_CODEGEN_LOG:-$PROBE_DIR/bootstrap-stage1-codegen.log}"
RUSTC_OUTPUT="${RISTUX_RUSTC_OUTPUT:-}"
HOST_RISTUX_LD_DIR="$PROBE_DIR/host-tools"
RUSTC_HOST="${RISTUX_HOST_RUSTC:-rustc +nightly}"
CARGO_STAGE0="${RISTUX_STAGE0_CARGO:-cargo +1.96.0}"
IFS=' ' read -r -a RUSTC_HOST_CMD <<< "$RUSTC_HOST"
IFS=' ' read -r -a CARGO_STAGE0_CMD <<< "$CARGO_STAGE0"

export RISTUX_RUST_TARGET_PROBE_DIR="$PROBE_DIR"

scripts/probe_rust_target.sh --prepare-only

source_dir="$PROBE_DIR/rustc-${RUST_VERSION}-src"
config="$PROBE_DIR/bootstrap.ristux.toml"

if [[ ! -f "$source_dir/x.py" ]]; then
  echo "prepared official Rust source is missing x.py: $source_dir" >&2
  exit 1
fi
if [[ ! -f "$config" ]]; then
  echo "prepared Ristux bootstrap config is missing: $config" >&2
  exit 1
fi

patch_static_cranelift_compiler() {
  local source_dir="$1"

  python3 - "$source_dir" <<'PY'
import pathlib
import sys
import hashlib
import json

root = pathlib.Path(sys.argv[1])

def replace(path, old, new, desc):
    text = path.read_text()
    if new in text:
        return
    if old not in text:
        raise SystemExit(f"failed to patch {desc}: pattern not found in {path}")
    path.write_text(text.replace(old, new, 1))

def insert_before(path, marker, addition, desc):
    text = path.read_text()
    if addition.strip() in text:
        return
    if marker not in text:
        raise SystemExit(f"failed to patch {desc}: marker not found in {path}")
    path.write_text(text.replace(marker, addition + marker, 1))

def refresh_vendor_checksum(crate_dir, *relative_paths):
    checksum_path = crate_dir / ".cargo-checksum.json"
    if not checksum_path.exists():
        return
    data = json.loads(checksum_path.read_text())
    files = data.setdefault("files", {})
    for rel in relative_paths:
        files[rel] = hashlib.sha256((crate_dir / rel).read_bytes()).hexdigest()
    checksum_path.write_text(json.dumps(data, sort_keys=True, separators=(",", ":")) + "\n")

rustc_driver_toml = root / "compiler/rustc_driver/Cargo.toml"
replace(
    rustc_driver_toml,
    'crate-type = ["dylib"]',
    'crate-type = ["rlib"]',
    "rustc_driver static crate type",
)

rustc_toml = root / "compiler/rustc/Cargo.toml"
replace(
    rustc_toml,
    'rustc_codegen_ssa = { path = "../rustc_codegen_ssa" }\n',
    'rustc_codegen_ssa = { path = "../rustc_codegen_ssa" }\n'
    'rustc_codegen_cranelift = { path = "../rustc_codegen_cranelift" }\n',
    "rustc-main Cranelift dependency",
)
replace(
    rustc_toml,
    'rustc_driver_impl = { path = "../rustc_driver_impl" }\n',
    'rustc_driver_impl = { path = "../rustc_driver_impl" }\n'
    'rustc_interface = { path = "../rustc_interface" }\n',
    "rustc-main rustc_interface dependency",
)

rustc_main = root / "compiler/rustc/src/main.rs"
replace(
    rustc_main,
    'fn main() -> ExitCode {\n    rustc_driver::main()\n}\n',
    '''struct RistuxCallbacks;

impl rustc_driver::Callbacks for RistuxCallbacks {
    fn config(&mut self, config: &mut rustc_interface::interface::Config) {
        config.opts.trimmed_def_paths = true;
        config.opts.unstable_opts.codegen_backend = None;
        config.make_codegen_backend = Some(Box::new(|_| {
            rustc_codegen_cranelift::__rustc_codegen_backend()
        }));
    }
}

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();
    let mut callbacks = RistuxCallbacks;
    rustc_driver::catch_with_exit_code(|| rustc_driver::run_compiler(&args, &mut callbacks))
}
''',
    "rustc-main static Cranelift entrypoint",
)

cg_toml = root / "compiler/rustc_codegen_cranelift/Cargo.toml"
replace(
    cg_toml,
    'crate-type = ["dylib"]',
    'crate-type = ["rlib"]',
    "Cranelift rlib crate type",
)
insert_before(
    cg_toml,
    "\n[patch.crates-io]\n",
    """
rustc_abi = { path = "../rustc_abi" }
rustc_ast = { path = "../rustc_ast" }
rustc_codegen_ssa = { path = "../rustc_codegen_ssa" }
rustc_const_eval = { path = "../rustc_const_eval" }
rustc_data_structures = { path = "../rustc_data_structures" }
rustc_driver = { path = "../rustc_driver" }
rustc_errors = { path = "../rustc_errors" }
rustc_fs_util = { path = "../rustc_fs_util" }
rustc_hir = { path = "../rustc_hir" }
rustc_incremental = { path = "../rustc_incremental" }
rustc_index = { path = "../rustc_index" }
rustc_log = { path = "../rustc_log" }
rustc_middle = { path = "../rustc_middle" }
rustc_session = { path = "../rustc_session" }
rustc_span = { path = "../rustc_span" }
rustc_symbol_mangling = { path = "../rustc_symbol_mangling" }
rustc_target = { path = "../rustc_target" }
""",
    "Cranelift explicit rustc_private dependencies",
)

symbol_rs = root / "compiler/rustc_span/src/symbol.rs"
replace(
    symbol_rs,
    "        x86_amx_intrinsics,\n        x87_reg,\n",
    "        x86_amx_intrinsics,\n        x87,\n        x87_reg,\n",
    "preinterned x87 symbol",
)

cg_lib = root / "compiler/rustc_codegen_cranelift/src/lib.rs"
replace(cg_lib, "use rustc_span::{Symbol, sym};", "use rustc_span::sym;", "Cranelift sym import")
replace(
    cg_lib,
    'vec![sym::fxsr, sym::sse, sym::sse2, Symbol::intern("x87")]',
    "vec![sym::fxsr, sym::sse, sym::sse2, sym::x87]",
    "Cranelift x87 target feature symbol",
)

aot_rs = root / "compiler/rustc_codegen_cranelift/src/driver/aot.rs"
replace(
    aot_rs,
    '''    rustc_codegen_ssa::assert_module_sources::assert_module_sources(tcx, &|cgu_reuse_tracker| {
        for (i, cgu) in cgus.iter().enumerate() {
            let cgu_reuse = cgu_reuse[i];
            cgu_reuse_tracker.set_actual_reuse(cgu.name().as_str(), cgu_reuse);
        }
    });
''',
    '''    if tcx.sess.opts.incremental.is_some() {
        rustc_codegen_ssa::assert_module_sources::assert_module_sources(
            tcx,
            &|cgu_reuse_tracker| {
                for (i, cgu) in cgus.iter().enumerate() {
                    let cgu_reuse = cgu_reuse[i];
                    cgu_reuse_tracker.set_actual_reuse(cgu.name().as_str(), cgu_reuse);
                }
            },
        );
    }
''',
    "Cranelift non-incremental module-source assertion",
)

compile_rs = root / "src/bootstrap/src/core/build_steps/compile.rs"
replace(
    compile_rs,
    '''                    CodegenBackendKind::Cranelift => {
                        let stamp = builder
                            .ensure(CraneliftCodegenBackend { compilers: prepare_compilers() });
                        copy_codegen_backends_to_sysroot(builder, stamp, target_compiler);
                    }
''',
    '''                    CodegenBackendKind::Cranelift => {
                        continue;
                    }
''',
    "bootstrap dynamic Cranelift component install",
)

driver_impl = root / "compiler/rustc_driver_impl/src/lib.rs"
replace(
    driver_impl,
    "pub fn version_at_macro_invocation(\n    early_dcx: &EarlyDiagCtxt,\n",
    "pub fn version_at_macro_invocation(\n    _early_dcx: &EarlyDiagCtxt,\n",
    "rustc -vV static backend diagnostics parameter",
)
replace(
    driver_impl,
    "        get_backend_from_raw_matches(early_dcx, matches).print_version();",
    '        safe_println!("Cranelift version: statically linked");',
    "rustc -vV static backend diagnostics",
)

cargo_toml = root / "src/tools/cargo/Cargo.toml"
replace(
    cargo_toml,
    'flate2 = { version = "1.1.9", default-features = false, features = ["zlib-rs"] }',
    'flate2 = { version = "1.1.9", default-features = false, features = ["rust_backend"] }',
    "Cargo flate2 pure Rust backend",
)

patched_crc32fast = 0
for crate_dir in sorted((root / "vendor").glob("crc32fast-*")):
    mod_rs = crate_dir / "src/specialized/mod.rs"
    if not mod_rs.exists():
        continue
    text = mod_rs.read_text()
    new_text = text.replace(
        '''    if #[cfg(all(
        target_feature = "sse2",
''',
        '''    if #[cfg(all(
        not(target_os = "ristux"),
        target_feature = "sse2",
''',
    ).replace(
        '''    if #[cfg(all(
        crc32fast_stdarchx86,
        target_feature = "sse2",
''',
        '''    if #[cfg(all(
        not(target_os = "ristux"),
        crc32fast_stdarchx86,
        target_feature = "sse2",
''',
    ).replace(
        '''    } else if #[cfg(all(stable_arm_crc32_intrinsics, target_arch = "aarch64"))] {
''',
        '''    } else if #[cfg(all(
        not(target_os = "ristux"),
        stable_arm_crc32_intrinsics,
        target_arch = "aarch64"
    ))] {
''',
    ).replace(
        '''    } else if #[cfg(all(feature = "nightly", target_arch = "aarch64"))] {
''',
        '''    } else if #[cfg(all(
        not(target_os = "ristux"),
        feature = "nightly",
        target_arch = "aarch64"
    ))] {
''',
    )
    if new_text != text:
        mod_rs.write_text(new_text)
        refresh_vendor_checksum(crate_dir, "src/specialized/mod.rs")
        patched_crc32fast += 1

if patched_crc32fast == 0:
    raise SystemExit("failed to patch vendored crc32fast crates for Ristux baseline CRC")

patched_sha1 = 0
for crate_dir in sorted((root / "vendor").glob("sha1-*")):
    compress_rs = crate_dir / "src/compress.rs"
    if not compress_rs.exists():
        continue
    text = compress_rs.read_text()
    new_text = text.replace(
        'if #[cfg(feature = "force-soft")] {',
        'if #[cfg(any(feature = "force-soft", target_os = "ristux"))] {',
    ).replace(
        'if #[cfg(sha1_backend = "soft")] {',
        'if #[cfg(any(sha1_backend = "soft", target_os = "ristux"))] {',
    )
    if new_text != text:
        compress_rs.write_text(new_text)
        refresh_vendor_checksum(crate_dir, "src/compress.rs")
        patched_sha1 += 1

if patched_sha1 == 0:
    raise SystemExit("failed to patch vendored sha1 crates for Ristux soft backend")

patched_sha2 = 0
for crate_dir in sorted((root / "vendor").glob("sha2-*")):
    changed = []
    for path_name in ("src/sha256.rs", "src/sha512.rs"):
        path = crate_dir / path_name
        if not path.exists():
            continue
        text = path.read_text()
        new_text = text.replace(
            'if #[cfg(feature = "force-soft")] {',
            'if #[cfg(any(feature = "force-soft", target_os = "ristux"))] {',
        ).replace(
            'if #[cfg(feature = "force-soft-compact")] {',
            'if #[cfg(all(feature = "force-soft-compact", not(target_os = "ristux")))] {',
        ).replace(
            'if #[cfg(any(sha2_backend = "soft", sha2_256_backend = "soft"))] {',
            'if #[cfg(any(target_os = "ristux", sha2_backend = "soft", sha2_256_backend = "soft"))] {',
        )
        if new_text != text:
            path.write_text(new_text)
            changed.append(path_name)
    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_sha2 += 1

if patched_sha2 == 0:
    raise SystemExit("failed to patch vendored sha2 crates for Ristux soft backend")

patched_libm = 0
for crate_dir in sorted((root / "vendor").glob("libm-*")):
    changed = []
    for toml_name in ("Cargo.toml", "Cargo.toml.orig"):
        toml = crate_dir / toml_name
        if not toml.exists():
            continue
        text = toml.read_text()
        new_text = text.replace('default = ["arch"]', 'default = []')
        if new_text != text:
            toml.write_text(new_text)
            changed.append(toml_name)
    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_libm += 1

if patched_libm == 0:
    raise SystemExit("failed to patch vendored libm crates for Ristux portable backend")

patched_blake3 = 0
for crate_dir in sorted((root / "vendor").glob("blake3-*")):
    build_rs = crate_dir / "build.rs"
    if not build_rs.exists():
        continue
    text = build_rs.read_text()
    new_text = text.replace(
        '''fn is_pure() -> bool {
    defined("CARGO_FEATURE_PURE")
}
''',
        '''fn is_pure() -> bool {
    defined("CARGO_FEATURE_PURE")
        || env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ristux")
}
''',
    )
    if new_text != text:
        build_rs.write_text(new_text)
        refresh_vendor_checksum(crate_dir, "build.rs")
        patched_blake3 += 1

if patched_blake3 == 0:
    raise SystemExit("failed to patch vendored blake3 crates for Ristux non-assembly backend")

patched_constant_time_eq = 0
for crate_dir in sorted((root / "vendor").glob("constant_time_eq-*")):
    changed = []
    for path in sorted((crate_dir / "src").rglob("*.rs")):
        text = path.read_text()
        new_text = text.replace(
            '#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]',
            '#[cfg(all(any(target_arch = "x86", target_arch = "x86_64"), not(target_os = "ristux")))]',
        ).replace(
            'any(target_arch = "x86", target_arch = "x86_64"),\n    target_feature = "sse2",',
            'any(target_arch = "x86", target_arch = "x86_64"),\n    not(target_os = "ristux"),\n    target_feature = "sse2",',
        ).replace(
            '''#[cfg(all(
    not(miri),
    any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "s390x",
    )
))]''',
            '''#[cfg(all(
    not(target_os = "ristux"),
    not(miri),
    any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "s390x",
    )
))]''',
        ).replace(
            '''#[cfg(any(
    miri,
    not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "s390x",
    ))
))]''',
            '''#[cfg(any(
    target_os = "ristux",
    miri,
    not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "arm64ec",
        target_arch = "riscv32",
        target_arch = "riscv64",
        target_arch = "loongarch64",
        target_arch = "s390x",
    ))
))]''',
        ).replace(
            '''#[cfg(any(
    not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "riscv32",
        target_arch = "riscv64",
    )),
    miri,
))]''',
            '''#[cfg(any(
    target_os = "ristux",
    not(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "arm",
        target_arch = "aarch64",
        target_arch = "riscv32",
        target_arch = "riscv64",
    )),
    miri,
))]''',
        )
        if new_text != text:
            path.write_text(new_text)
            changed.append(str(path.relative_to(crate_dir)))
    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_constant_time_eq += 1

if patched_constant_time_eq == 0:
    raise SystemExit("failed to patch vendored constant_time_eq crates for Ristux generic backend")

patched_libloading = 0
for crate_dir in sorted((root / "vendor").glob("libloading-*")):
    consts_rs = crate_dir / "src/os/unix/consts.rs"
    if not consts_rs.exists():
        continue
    text = consts_rs.read_text()
    new_text = text.replace(
        '            target_os = "redox",\n            target_os = "nto",',
        '            target_os = "redox",\n            target_os = "ristux",\n            target_os = "nto",',
    ).replace(
        '            target_os = "redox",\n            target_os = "hurd",',
        '            target_os = "redox",\n            target_os = "ristux",\n            target_os = "hurd",',
    )
    if new_text != text:
        consts_rs.write_text(new_text)
        refresh_vendor_checksum(crate_dir, "src/os/unix/consts.rs")
        patched_libloading += 1

if patched_libloading == 0:
    raise SystemExit("failed to patch vendored libloading crates for Ristux RTLD constants")

patched_target_lexicon = 0
for crate_dir in sorted((root / "vendor").glob("target-lexicon-*")):
    changed = []
    targets_rs = crate_dir / "src/targets.rs"
    if targets_rs.exists():
        text = targets_rs.read_text()
        new_text = text.replace(
            '    Redox,\n    Solaris,',
            '    Redox,\n    Ristux,\n    Solaris,',
        ).replace(
            '            Redox => Cow::Borrowed("redox"),\n            Solaris => Cow::Borrowed("solaris"),',
            '            Redox => Cow::Borrowed("redox"),\n            Ristux => Cow::Borrowed("ristux"),\n            Solaris => Cow::Borrowed("solaris"),',
        ).replace(
            '            "redox" => Redox,\n            "solaris" => Solaris,',
            '            "redox" => Redox,\n            "ristux" => Ristux,\n            "solaris" => Solaris,',
        )
        if new_text != text:
            targets_rs.write_text(new_text)
            changed.append("src/targets.rs")
    triple_rs = crate_dir / "src/triple.rs"
    if triple_rs.exists():
        text = triple_rs.read_text()
        new_text = text.replace(
            '            | OperatingSystem::Redox\n            | OperatingSystem::Solaris => CallingConvention::SystemV,',
            '            | OperatingSystem::Redox\n            | OperatingSystem::Ristux\n            | OperatingSystem::Solaris => CallingConvention::SystemV,',
        )
        if new_text != text:
            triple_rs.write_text(new_text)
            changed.append("src/triple.rs")
    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_target_lexicon += 1

if patched_target_lexicon == 0:
    raise SystemExit("failed to patch vendored target-lexicon crates for Ristux target triples")

patched_nix = 0
for crate_dir in sorted((root / "vendor").glob("nix-*")):
    changed = []
    errno_rs = crate_dir / "src/errno.rs"
    if errno_rs.exists():
        text = errno_rs.read_text()
        new_text = text.replace(
            'target_os = "linux",\n                        target_os = "redox",\n                        target_os = "dragonfly",',
            'target_os = "linux",\n                        target_os = "redox",\n                        target_os = "ristux",\n                        target_os = "dragonfly",',
        ).replace(
            '#[cfg(target_os = "redox")]\nmod consts {',
            '#[cfg(any(target_os = "redox", target_os = "ristux"))]\nmod consts {',
        ).replace(
            '        #[cfg(target_os = "hurd")]\n        EGRATUITOUS => "Gratuitous error",\n    }\n}\n',
            '        #[cfg(target_os = "hurd")]\n        EGRATUITOUS => "Gratuitous error",\n\n        #[cfg(target_os = "ristux")]\n        _ => "Unknown errno",\n    }\n}\n',
        )
        if new_text != text:
            errno_rs.write_text(new_text)
            changed.append("src/errno.rs")

    signal_rs = crate_dir / "src/sys/signal.rs"
    if signal_rs.exists():
        text = signal_rs.read_text()
        new_text = text.replace(
            'target_os = "openbsd",\n    target_os = "redox"\n)))]',
            'target_os = "openbsd",\n    target_os = "redox",\n    target_os = "ristux"\n)))]',
        ).replace(
            'target_os = "redox", target_os = "haiku"',
            'target_os = "redox", target_os = "ristux", target_os = "haiku"',
        ).replace(
            'target_os = "redox",\n                target_os = "haiku"',
            'target_os = "redox",\n                target_os = "ristux",\n                target_os = "haiku"',
        ).replace(
            'target_os = "redox",\n                target_os = "aix"',
            'target_os = "redox",\n                target_os = "ristux",\n                target_os = "aix"',
        ).replace(
            'target_os = "redox",\n                      target_os = "haiku"',
            'target_os = "redox",\n                      target_os = "ristux",\n                      target_os = "haiku"',
        ).replace(
            '#[cfg(target_os = "redox")]\n#[cfg(feature = "signal")]\nconst SIGNALS: [Signal; 29] = [',
            '#[cfg(any(target_os = "redox", target_os = "ristux"))]\n#[cfg(feature = "signal")]\nconst SIGNALS: [Signal; 29] = [',
        ).replace(
            '    target_os = "redox",\n    target_os = "haiku",',
            '    target_os = "redox",\n    target_os = "ristux",\n    target_os = "haiku",',
        ).replace(
            'not(target_os = "redox")',
            'not(any(target_os = "redox", target_os = "ristux"))',
        ).replace(
            '        SA_NOCLDSTOP;',
            '        SA_NOCLDSTOP as SaFlags_t;',
        ).replace(
            '        SA_NOCLDWAIT;',
            '        SA_NOCLDWAIT as SaFlags_t;',
        ).replace(
            '        SA_NODEFER;',
            '        SA_NODEFER as SaFlags_t;',
        ).replace(
            '        SA_ONSTACK;',
            '        SA_ONSTACK as SaFlags_t;',
        ).replace(
            '        SA_RESETHAND;',
            '        SA_RESETHAND as SaFlags_t;',
        ).replace(
            '        SA_RESTART;',
            '        SA_RESTART as SaFlags_t;',
        ).replace(
            '        SA_SIGINFO;',
            '        SA_SIGINFO as SaFlags_t;',
        )
        if new_text != text:
            signal_rs.write_text(new_text)
            changed.append("src/sys/signal.rs")

    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_nix += 1

if patched_nix == 0:
    raise SystemExit("failed to patch vendored nix crates for Ristux Redox-shaped errno/signal ABI")

patched_ctrlc = 0
for crate_dir in sorted((root / "vendor").glob("ctrlc-*")):
    unix_rs = crate_dir / "src/platform/unix/mod.rs"
    if not unix_rs.exists():
        continue
    text = unix_rs.read_text()
    new_text = text.replace(
        '#[cfg(not(target_vendor = "apple"))]\n#[allow(static_mut_refs)] // rust-version = "1.69.0"\nmod implementation {',
        '#[cfg(not(any(target_vendor = "apple", target_os = "ristux")))]\n#[allow(static_mut_refs)] // rust-version = "1.69.0"\nmod implementation {',
    ).replace(
        '''#[cfg(target_vendor = "apple")]
mod implementation {''',
        '''#[cfg(target_os = "ristux")]
mod implementation {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    static SIGNALED: AtomicBool = AtomicBool::new(false);

    pub unsafe fn sem_init() {
        SIGNALED.store(false, Ordering::SeqCst);
    }

    pub unsafe fn sem_post() {
        SIGNALED.store(true, Ordering::SeqCst);
    }

    pub unsafe fn sem_wait_forever() {
        while !SIGNALED.swap(false, Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(target_vendor = "apple")]
mod implementation {''',
    )
    if new_text != text:
        unix_rs.write_text(new_text)
        refresh_vendor_checksum(crate_dir, "src/platform/unix/mod.rs")
        patched_ctrlc += 1

if patched_ctrlc == 0:
    raise SystemExit("failed to patch vendored ctrlc crates for Ristux non-POSIX-semaphore signal wait")

patched_zlib_rs = 0
for crate_dir in sorted((root / "vendor").glob("zlib-rs-*")):
    changed = []
    for path in sorted((crate_dir / "src").rglob("*.rs")):
        text = path.read_text()
        new_text = text.replace(
            '#[cfg(target_arch = "x86_64")]',
            '#[cfg(all(target_arch = "x86_64", not(target_os = "ristux")))]',
        ).replace(
            '#[cfg(any(target_arch = "x86_64", target_arch = "x86"))]',
            '#[cfg(all(any(target_arch = "x86_64", target_arch = "x86"), not(target_os = "ristux")))]',
        ).replace(
            'cfg!(all(target_feature = "avx512f", target_feature = "avx512bw"))',
            'cfg!(all(not(target_os = "ristux"), target_feature = "avx512f", target_feature = "avx512bw"))',
        ).replace(
            '#[cfg_attr(all(target_arch = "x86_64", target_feature = "avx2"), inline(never))]',
            '#[cfg_attr(all(target_arch = "x86_64", not(target_os = "ristux"), target_feature = "avx2"), inline(never))]',
        )
        if new_text != text:
            path.write_text(new_text)
            changed.append(str(path.relative_to(crate_dir)))
    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_zlib_rs += 1

if patched_zlib_rs == 0:
    raise SystemExit("failed to patch vendored zlib-rs crate for Ristux portable backend")

patched_getrandom = 0
for crate_dir in sorted((root / "vendor").glob("getrandom-*")):
    changed = []
    for toml_name in ("Cargo.toml", "Cargo.toml.orig"):
        toml = crate_dir / toml_name
        if not toml.exists():
            continue
        text = toml.read_text()
        new_text = text.replace(
            'target_os = "haiku", target_os = "redox", target_os = "nto", target_os = "aix"',
            'target_os = "haiku", target_os = "redox", target_os = "ristux", target_os = "nto", target_os = "aix"',
        )
        if new_text != text:
            toml.write_text(new_text)
            changed.append(toml_name)
    backends_rs = crate_dir / "src/backends.rs"
    if backends_rs.exists():
        text = backends_rs.read_text()
        new_text = text.replace(
            '        target_os = "redox",\n        target_os = "nto",',
            '        target_os = "redox",\n        target_os = "ristux",\n        target_os = "nto",',
        )
        if new_text != text:
            backends_rs.write_text(new_text)
            changed.append("src/backends.rs")
    get_errno_rs = crate_dir / "src/utils/get_errno.rs"
    if get_errno_rs.exists():
        text = get_errno_rs.read_text()
        new_text = text.replace(
            'target_os = "hurd", target_os = "redox", target_os = "dragonfly"',
            'target_os = "hurd", target_os = "redox", target_os = "ristux", target_os = "dragonfly"',
        )
        if new_text != text:
            get_errno_rs.write_text(new_text)
            changed.append("src/utils/get_errno.rs")
    util_libc_rs = crate_dir / "src/util_libc.rs"
    if util_libc_rs.exists():
        text = util_libc_rs.read_text()
        new_text = text.replace(
            'target_os = "linux", target_os = "emscripten", target_os = "hurd", target_os = "redox", target_os = "dragonfly"',
            'target_os = "linux", target_os = "emscripten", target_os = "hurd", target_os = "redox", target_os = "ristux", target_os = "dragonfly"',
        )
        if new_text != text:
            util_libc_rs.write_text(new_text)
            changed.append("src/util_libc.rs")
    if changed:
        refresh_vendor_checksum(crate_dir, *changed)
        patched_getrandom += 1

if patched_getrandom == 0:
    raise SystemExit("failed to patch vendored getrandom crates for Ristux file backend")

patched_rustix = 0
for crate_dir in sorted((root / "vendor").glob("rustix-*")):
    ioctl_rs = crate_dir / "src/ioctl/mod.rs"
    if not ioctl_rs.exists():
        continue
    text = ioctl_rs.read_text()
    new_text = text.replace(
        '    target_os = "redox",\n    target_os = "haiku",',
        '    target_os = "redox",\n    target_os = "ristux",\n    target_os = "haiku",',
    )
    if new_text != text:
        ioctl_rs.write_text(new_text)
        refresh_vendor_checksum(crate_dir, "src/ioctl/mod.rs")
        patched_rustix += 1

if patched_rustix == 0:
    raise SystemExit("failed to patch vendored rustix ioctl opcode type for Ristux")
PY

  (
    cd "$source_dir"
    "${CARGO_STAGE0_CMD[@]}" update --offline -p rustc-main
  ) >/dev/null
  (
    cd "$source_dir/src/tools/cargo"
    "${CARGO_STAGE0_CMD[@]}" update --offline -p flate2
  ) >/dev/null

  grep -q 'rustc_codegen_cranelift = { path = "../rustc_codegen_cranelift" }' "$source_dir/compiler/rustc/Cargo.toml" || {
    echo "failed to link Cranelift into rustc-main" >&2
    exit 1
  }
  grep -q 'config.make_codegen_backend' "$source_dir/compiler/rustc/src/main.rs" || {
    echo "failed to install static Cranelift rustc entrypoint" >&2
    exit 1
  }
  grep -q 'crate-type = \["rlib"\]' "$source_dir/compiler/rustc_codegen_cranelift/Cargo.toml" || {
    echo "failed to make rustc_codegen_cranelift static-only for Ristux bootstrap" >&2
    exit 1
  }
}

mkdir -p "$HOST_RISTUX_LD_DIR"
"${RUSTC_HOST_CMD[@]}" \
  --edition=2024 \
  --cfg ristux_ld_host \
  "$PWD/userland/src/bin/ristux_ld.rs" \
  -o "$HOST_RISTUX_LD_DIR/ristux-ld"
"$HOST_RISTUX_LD_DIR/ristux-ld" --self-test >/dev/null
"$HOST_RISTUX_LD_DIR/ristux-ld" --self-test-archive >/dev/null

mkdir -p "$(dirname "$STAGE1_CODEGEN_LOG")"
set +e
(
  cd "$source_dir"
  PATH="$HOST_RISTUX_LD_DIR:$PATH" \
    BOOTSTRAP_SKIP_TARGET_SANITY=1 \
    python3 x.py \
      --config "$config" \
      build \
      --stage 1 \
      --target x86_64-unknown-ristux \
      library/std
) > "$STAGE1_CODEGEN_LOG" 2>&1
stage1_status=$?
set -e

if [[ $stage1_status -ne 0 ]]; then
  echo "official Rust $RUST_VERSION stage1 Ristux std/bootstrap prebuild failed; tail of $STAGE1_CODEGEN_LOG:" >&2
  tail -120 "$STAGE1_CODEGEN_LOG" >&2
  exit "$stage1_status"
fi

patch_static_cranelift_compiler "$source_dir"

mkdir -p "$(dirname "$RUSTC_LOG")"
set +e
(
  cd "$source_dir"
  PATH="$HOST_RISTUX_LD_DIR:$PATH" \
    BOOTSTRAP_SKIP_TARGET_SANITY=1 \
    python3 x.py \
      --config "$config" \
      build \
      --stage 2 \
      --host x86_64-unknown-ristux \
      --target x86_64-unknown-ristux \
      compiler/rustc
) > "$RUSTC_LOG" 2>&1
rustc_status=$?
set -e

if [[ $rustc_status -eq 0 ]]; then
  rustc_bins=()
  while IFS= read -r rustc_bin; do
    rustc_bins+=("$rustc_bin")
  done < <(find "$PROBE_DIR/bootstrap-build" -type f -path '*/x86_64-unknown-ristux/stage2/bin/rustc' -print | sort)
  if [[ ${#rustc_bins[@]} -eq 0 ]]; then
    echo "official Rust $RUST_VERSION stage2 Ristux rustc build succeeded but did not produce expected rustc binary" >&2
    echo "log: $RUSTC_LOG" >&2
    exit 1
  fi
  if [[ -n "$RUSTC_OUTPUT" ]]; then
    mkdir -p "$(dirname "$RUSTC_OUTPUT")"
    cp "${rustc_bins[0]}" "$RUSTC_OUTPUT"
    chmod 755 "$RUSTC_OUTPUT"
  fi
else
  if grep -q 'cannot produce dylib for `rustc_driver' "$RUSTC_LOG"; then
    echo "official Rust $RUST_VERSION stage2 Ristux rustc build is still blocked by rustc_driver dylib output despite static patch; tail of $RUSTC_LOG:" >&2
    tail -120 "$RUSTC_LOG" >&2
    exit "$rustc_status"
  fi

  if grep -q 'required to be available in rlib format' "$RUSTC_LOG" \
    && grep -q 'could not compile `rustc_codegen_cranelift`' "$RUSTC_LOG"; then
    echo "official Rust $RUST_VERSION stage2 Ristux rustc build regressed to the old static codegen-backend dependency-format blocker; tail of $RUSTC_LOG:" >&2
    tail -120 "$RUSTC_LOG" >&2
    exit "$rustc_status"
  fi

  if grep -q 'No space left on device' "$RUSTC_LOG"; then
    echo "official Rust $RUST_VERSION stage2 Ristux rustc build ran out of local disk space; tail of $RUSTC_LOG:" >&2
    tail -120 "$RUSTC_LOG" >&2
    exit "$rustc_status"
  fi

  echo "official Rust $RUST_VERSION stage2 Ristux rustc build failed with an unexpected blocker; tail of $RUSTC_LOG:" >&2
  tail -120 "$RUSTC_LOG" >&2
  exit "$rustc_status"
fi

mkdir -p "$(dirname "$LOG")"
set +e
(
  cd "$source_dir"
  PATH="$HOST_RISTUX_LD_DIR:$PATH" \
    BOOTSTRAP_SKIP_TARGET_SANITY=1 \
    python3 x.py \
      --config "$config" \
      build \
      --stage 2 \
      --host x86_64-unknown-ristux \
      --target x86_64-unknown-ristux \
      cargo
) > "$LOG" 2>&1
status=$?
set -e

if [[ $status -eq 0 ]]; then
  cargo_bins=()
  while IFS= read -r cargo_bin; do
    cargo_bins+=("$cargo_bin")
  done < <(find "$PROBE_DIR/bootstrap-build" -type f \( -path '*/stage2-tools-bin/cargo' -o -path '*/stage2-tools/x86_64-unknown-ristux/release/cargo' \) -print)
  if [[ ${#cargo_bins[@]} -eq 0 ]]; then
    echo "official Rust $RUST_VERSION stage2 Cargo build succeeded but did not produce expected Ristux Cargo binary" >&2
    echo "log: $LOG" >&2
    exit 1
  fi
  echo "official Rust $RUST_VERSION stage2 Ristux rustc/Cargo bootstrap build passed: $RUSTC_LOG ; $LOG"
  exit 0
fi

if grep -q 'cannot produce dylib for `rustc_driver' "$LOG"; then
  echo "official Rust $RUST_VERSION stage2 probe is still blocked by rustc_driver dylib output despite static patch; tail of $LOG:" >&2
  tail -120 "$LOG" >&2
  exit "$status"
fi

if grep -q 'required to be available in rlib format' "$LOG" \
  && grep -q 'could not compile `rustc_codegen_cranelift`' "$LOG"; then
  echo "official Rust $RUST_VERSION stage2 probe regressed to the old static codegen-backend dependency-format blocker; tail of $LOG:" >&2
  tail -120 "$LOG" >&2
  exit "$status"
fi

if grep -q 'No space left on device' "$LOG"; then
  echo "official Rust $RUST_VERSION stage2 probe reached the Ristux Cargo tool build but ran out of local disk space; tail of $LOG:" >&2
  tail -120 "$LOG" >&2
  exit "$status"
fi

if { grep -q 'Building stage2 tool cargo' "$LOG" \
    || grep -q 'Building stage2 cargo' "$LOG" \
    || grep -q 'stage2-tools/x86_64-unknown-ristux' "$LOG"; } \
  && { grep -q 'could not compile `zlib-rs`' "$LOG" \
    || grep -q 'could not compile `sha1`' "$LOG" \
    || grep -q 'could not compile `libz-sys`' "$LOG" \
    || grep -q 'could not compile `curl-sys`' "$LOG" \
    || grep -q 'could not compile `libgit2-sys`' "$LOG" \
    || grep -q 'fatal error: .*file not found' "$LOG" \
    || grep -q 'error occurred in cc-rs' "$LOG" \
    || grep -q 'command did not execute successfully.*--target=x86_64-unknown-ristux.*"-c".*\\.c' "$LOG" \
    || grep -q 'target is not supported' "$LOG" \
    || grep -q 'unresolved imports.*F_RDLCK' "$LOG" \
    || grep -q 'could not compile `getrandom`' "$LOG" \
    || grep -q 'could not compile `rustix`' "$LOG"; }; then
  echo "official Rust $RUST_VERSION stage2 Ristux rustc build passed; Cargo reached C-backed dependency blockers: $RUSTC_LOG ; $LOG"
  exit 0
fi

grep -Eq 'Building stage2 (tool )?cargo' "$LOG" || {
  echo "official Rust $RUST_VERSION stage2 probe did not reach Ristux Cargo tool build; tail of $LOG:" >&2
  tail -120 "$LOG" >&2
  exit "$status"
}

echo "official Rust $RUST_VERSION stage2 Ristux Cargo bootstrap failed with an unexpected blocker; tail of $LOG:" >&2
tail -120 "$LOG" >&2
exit "$status"
