use crate::spec::{
    Arch, Cc, LinkerFlavor, Lld, Os, PanicStrategy, RelocModel, RustcAbi, Target, TargetMetadata,
    TargetOptions, cvs,
};

pub(crate) fn target() -> Target {
    let opts = TargetOptions {
        os: Os::Ristux,
        families: cvs!["unix"],
        cpu: "x86-64".into(),
        plt_by_default: false,
        max_atomic_width: Some(64),
        linker_flavor: LinkerFlavor::Gnu(Cc::No, Lld::No),
        linker: Some("ristux-ld".into()),
        rustc_abi: Some(RustcAbi::Softfloat),
        features: "-mmx,-sse,-sse2,-sse3,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,+soft-float".into(),
        disable_redzone: true,
        panic_strategy: PanicStrategy::Abort,
        relocation_model: RelocModel::Static,
        ..Default::default()
    };

    Target {
        llvm_target: "x86_64-unknown-none-elf".into(),
        metadata: TargetMetadata {
            description: Some("64-bit Ristux".into()),
            tier: Some(3),
            host_tools: Some(true),
            std: Some(true),
        },
        pointer_width: 64,
        data_layout:
            "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128".into(),
        arch: Arch::X86_64,
        options: opts,
    }
}
