//! Platform capability gates that span multiple feature modules (not
//! specific to local STT or local LLM alone).

/// True only for a native Intel (x86_64) macOS build — never Windows, never
/// Apple Silicon macOS.
///
/// Local on-device STT/LLM inference (`local_stt`, `local_llm`) is
/// deliberately gated off entirely on this platform: it has zero real-world
/// testing (neither the maintainer nor any current tester owns an Intel
/// Mac), and Intel Macs are old enough now that they're both increasingly
/// uncommon and generally underpowered for local LLM inference specifically
/// — running it there for the first time on a real user's machine, unable to
/// know whether it works at all, is a worse outcome than clearly saying "not
/// yet" and pointing at cloud providers instead. Revisit once there's an
/// actual Intel Mac to validate against (see docs/ROADMAP.md).
pub fn is_macos_intel() -> bool {
    cfg!(all(target_os = "macos", not(target_arch = "aarch64")))
}

#[cfg(test)]
mod tests {
    use super::is_macos_intel;

    #[test]
    fn is_false_on_every_platform_except_intel_macos() {
        // This is a compile-time cfg check, so the only meaningful assertion
        // a single build can make is about itself. On Windows and Apple
        // Silicon macOS CI runners (and this crate's actual local dev/test
        // machine) it must be false; the true case only compiles in on a
        // real x86_64-apple-darwin target, which no CI runner in this
        // project's matrix currently builds/runs as a native target.
        #[cfg(not(all(target_os = "macos", not(target_arch = "aarch64"))))]
        assert!(!is_macos_intel());
    }
}
