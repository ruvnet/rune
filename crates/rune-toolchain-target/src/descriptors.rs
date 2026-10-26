//! Target descriptor constructors (ADR 006).

use rune_core::target::TargetDescriptor;
use std::path::Path;

pub const X86_64_LINUX: &str = "x86_64-unknown-linux-gnu";
pub const AARCH64_LINUX: &str = "aarch64-unknown-linux-gnu";
pub const WASM32_WASI: &str = "wasm32-wasip1";
pub const RVM: &str = "rvm";

/// Why RVM can never be validated today: it has no ABI, instruction or
/// capability specification, and none is invented here.
pub const RVM_UNVALIDATED_REASON: &str = "specification required";

pub const RUNTIME_NATIVE: &str = "native";
pub const RUNTIME_QEMU_AARCH64: &str = "qemu-aarch64-static";
pub const RUNTIME_NODE_WASI: &str = "node-wasi";
pub const RUNTIME_UNAVAILABLE: &str = "unavailable";

const AARCH64_SYSROOT: &str = "/usr/aarch64-linux-gnu";

/// Native x86_64 Linux. `required_features` are CPU flags the artifact needs
/// (e.g. `avx2`); loading fails on a host without all of them.
pub fn x86_64_linux_gnu(required_features: &[&str]) -> TargetDescriptor {
    TargetDescriptor {
        triple: X86_64_LINUX.into(),
        cpu_features: required_features.iter().map(|s| (*s).into()).collect(),
        os: "linux".into(),
        linker: "cc".into(),
        sysroot: None,
        runtime: RUNTIME_NATIVE.into(),
        abi: "sysv-x86_64".into(),
    }
}

/// aarch64 Linux, linked with the GNU cross gcc and run under qemu-user
/// (emulated: correctness evidence only). The sysroot is recorded only when
/// present so the descriptor never names a path that does not exist.
pub fn aarch64_linux_gnu(required_features: &[&str]) -> TargetDescriptor {
    let sysroot = Path::new(AARCH64_SYSROOT)
        .is_dir()
        .then(|| AARCH64_SYSROOT.to_owned());
    TargetDescriptor {
        triple: AARCH64_LINUX.into(),
        cpu_features: required_features.iter().map(|s| (*s).into()).collect(),
        os: "linux".into(),
        linker: "aarch64-linux-gnu-gcc".into(),
        sysroot,
        runtime: RUNTIME_QEMU_AARCH64.into(),
        abi: "aapcs64".into(),
    }
}

/// wasm32 WASI preview1, executed by Node's `node:wasi` (a foreign runtime).
pub fn wasm32_wasip1() -> TargetDescriptor {
    TargetDescriptor {
        triple: WASM32_WASI.into(),
        cpu_features: Vec::new(),
        os: "wasi".into(),
        linker: "rust-lld".into(),
        sysroot: None,
        runtime: RUNTIME_NODE_WASI.into(),
        abi: "wasi-preview1".into(),
    }
}

/// RVM placeholder. Unavailable by construction: no ABI exists to describe.
pub fn rvm() -> TargetDescriptor {
    TargetDescriptor {
        triple: RVM.into(),
        cpu_features: Vec::new(),
        os: "rvm".into(),
        linker: String::new(),
        sysroot: None,
        runtime: RUNTIME_UNAVAILABLE.into(),
        abi: "unspecified".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_differ_per_target_and_feature() {
        let a = x86_64_linux_gnu(&["sse2"]).digest();
        assert_ne!(a, x86_64_linux_gnu(&["avx2"]).digest());
        assert_ne!(a, aarch64_linux_gnu(&[]).digest());
        assert_ne!(wasm32_wasip1().digest(), rvm().digest());
    }
}
