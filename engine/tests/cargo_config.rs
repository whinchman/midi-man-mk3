/// Tests for BUG-003: hardcoded /tmp ALSA paths must not appear as live config
/// values in `.cargo/config.toml`, and `.gitignore` must contain the entry for
/// the gitignored local-override file.
///
/// `CARGO_MANIFEST_DIR` is set by Cargo to the engine crate root at test time.
/// The workspace root (where `.cargo/config.toml` and `.gitignore` live) is one
/// directory above.

use std::path::PathBuf;

/// Return the workspace root directory (one level above the engine crate root).
fn workspace_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir).parent().expect("engine crate must have a parent directory").to_path_buf()
}

/// Read `.cargo/config.toml` from the workspace root and return all non-comment,
/// non-blank lines. A line is a comment line if its first non-whitespace
/// character is `#`.
fn live_config_lines() -> Vec<String> {
    let path = workspace_root().join(".cargo/config.toml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(str::to_owned)
        .collect()
}

/// Read the full text of `.cargo/config.toml` (including comment lines).
fn full_cargo_config() -> String {
    let path = workspace_root().join(".cargo/config.toml");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

/// Read the full text of `.gitignore`.
fn gitignore_contents() -> String {
    let path = workspace_root().join(".gitignore");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
}

// ---------------------------------------------------------------------------
// T1 – no live (non-comment) line in .cargo/config.toml contains /tmp/alsa-pkg
// ---------------------------------------------------------------------------

#[test]
fn cargo_config_no_live_tmp_alsa_pkg() {
    let live_lines = live_config_lines();
    for line in &live_lines {
        assert!(
            !line.contains("/tmp/alsa-pkg"),
            "live config line must not reference /tmp/alsa-pkg (BUG-003): found '{line}'"
        );
    }
}

// ---------------------------------------------------------------------------
// T2 – no live (non-comment) line in .cargo/config.toml contains /tmp/alsa-lib
// ---------------------------------------------------------------------------

#[test]
fn cargo_config_no_live_tmp_alsa_lib() {
    let live_lines = live_config_lines();
    for line in &live_lines {
        assert!(
            !line.contains("/tmp/alsa-lib"),
            "live config line must not reference /tmp/alsa-lib (BUG-003): found '{line}'"
        );
    }
}

// ---------------------------------------------------------------------------
// T3 – .gitignore contains .cargo/config.local.toml
// ---------------------------------------------------------------------------

#[test]
fn gitignore_contains_config_local_toml() {
    let contents = gitignore_contents();
    let has_entry = contents
        .lines()
        .any(|line| line.trim() == ".cargo/config.local.toml");
    assert!(
        has_entry,
        ".gitignore must contain a '.cargo/config.local.toml' entry so local overrides are never committed"
    );
}

// ---------------------------------------------------------------------------
// T4 – .cargo/config.toml documents the local-override pattern in a comment
// ---------------------------------------------------------------------------

#[test]
fn cargo_config_documents_local_override_pattern() {
    let contents = full_cargo_config();
    assert!(
        contents.contains(".cargo/config.local.toml"),
        ".cargo/config.toml must mention '.cargo/config.local.toml' in a comment to document the local-override pattern for developers"
    );
}
