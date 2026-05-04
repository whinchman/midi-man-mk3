/// Tests that validate the workspace Cargo.toml profile configuration
/// introduced in the fix-cargo-firmware-debug task.
///
/// These tests read the workspace Cargo.toml at test time and verify:
///   - `[profile.release.package.firmware]` with `debug = false` is present.
///   - No `[profile.release.package.engine]` override exists (engine retains
///     workspace-level `debug = 2`).
///   - The workspace `[profile.release]` section retains `debug = 2`.
///
/// Tests use line-based string search rather than a TOML parser to avoid a
/// dev-dependency, and use CARGO_MANIFEST_DIR to locate the workspace root
/// reliably regardless of the working directory at test invocation time.
use std::path::PathBuf;

/// Returns the workspace root by traversing up from CARGO_MANIFEST_DIR
/// (which points at engine/) until we find the file that contains
/// `[workspace]`.
fn workspace_cargo_toml() -> PathBuf {
    // CARGO_MANIFEST_DIR is set by cargo test to the crate being tested.
    // For this crate that is <workspace>/engine.
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo");
    let mut path = PathBuf::from(manifest_dir);

    // Walk up until we find a Cargo.toml that contains `[workspace]`.
    loop {
        let candidate = path.join("Cargo.toml");
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate).unwrap_or_default();
            if content.contains("[workspace]") {
                return candidate;
            }
        }
        match path.parent() {
            Some(parent) => path = parent.to_path_buf(),
            None => panic!("could not locate workspace Cargo.toml"),
        }
    }
}

/// Helper: read the workspace Cargo.toml content as a String.
fn read_workspace_cargo_toml() -> String {
    let path = workspace_cargo_toml();
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

// ---------------------------------------------------------------------------
// 1. File existence
// ---------------------------------------------------------------------------

#[test]
fn workspace_cargo_toml_exists() {
    let path = workspace_cargo_toml();
    assert!(
        path.exists(),
        "workspace Cargo.toml not found at expected location: {}",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// 2. firmware release profile override is present
// ---------------------------------------------------------------------------

#[test]
fn firmware_release_profile_section_present() {
    let content = read_workspace_cargo_toml();
    assert!(
        content.contains("[profile.release.package.firmware]"),
        "workspace Cargo.toml must contain [profile.release.package.firmware] table"
    );
}

#[test]
fn firmware_release_profile_has_debug_false() {
    let content = read_workspace_cargo_toml();

    // Find the firmware override section and verify `debug = false` follows it
    // before the next section header begins.
    let section_start = content
        .find("[profile.release.package.firmware]")
        .expect("section [profile.release.package.firmware] not found in workspace Cargo.toml");

    let after_section = &content[section_start..];

    // The `debug = false` must appear before the next `[` heading (or end of file).
    let end_of_section = after_section[1..] // skip the opening `[` of this section
        .find('[')
        .map(|idx| idx + 1) // offset back relative to after_section
        .unwrap_or(after_section.len());

    let section_body = &after_section[..end_of_section];

    assert!(
        section_body.contains("debug = false"),
        "expected `debug = false` inside [profile.release.package.firmware], \
         but section body was:\n{}",
        section_body
    );
}

// ---------------------------------------------------------------------------
// 3. Engine has no per-package release profile override
// ---------------------------------------------------------------------------

#[test]
fn engine_has_no_release_package_override() {
    let content = read_workspace_cargo_toml();
    assert!(
        !content.contains("[profile.release.package.engine]"),
        "workspace Cargo.toml must NOT contain [profile.release.package.engine] — \
         engine must inherit debug = 2 from the workspace release profile"
    );
}

// ---------------------------------------------------------------------------
// 4. Workspace release profile retains debug = 2
// ---------------------------------------------------------------------------

#[test]
fn workspace_release_profile_debug_is_2() {
    let content = read_workspace_cargo_toml();

    // Find the workspace-level [profile.release] section (not the firmware sub-section).
    // We look for a line that is exactly `[profile.release]` (no `.package.`).
    let section_start = content
        .find("[profile.release]\n")
        .expect("workspace [profile.release] section not found");

    let after_section = &content[section_start..];

    // Grab content up to the next `[` heading (the firmware override or EOF).
    let end_of_section = after_section[1..]
        .find('[')
        .map(|idx| idx + 1)
        .unwrap_or(after_section.len());

    let section_body = &after_section[..end_of_section];

    assert!(
        section_body.contains("debug = 2"),
        "workspace [profile.release] must retain `debug = 2` for engine profiling; \
         section body was:\n{}",
        section_body
    );
}

// ---------------------------------------------------------------------------
// 5. firmware debug override does not bleed into engine profile
// ---------------------------------------------------------------------------

#[test]
fn firmware_override_does_not_affect_engine_profile() {
    // Confirm that there is exactly one per-package firmware override and zero
    // per-package engine overrides.  This is a combined check of tests 3 & 2.
    let content = read_workspace_cargo_toml();

    let firmware_overrides = content
        .matches("[profile.release.package.firmware]")
        .count();
    assert_eq!(
        firmware_overrides, 1,
        "expected exactly one [profile.release.package.firmware] section, found {}",
        firmware_overrides
    );

    let engine_overrides = content.matches("[profile.release.package.engine]").count();
    assert_eq!(
        engine_overrides, 0,
        "expected zero [profile.release.package.engine] sections, found {}",
        engine_overrides
    );
}
