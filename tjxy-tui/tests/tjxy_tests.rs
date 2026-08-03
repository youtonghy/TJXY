use std::path::Path;

use tjxy_tui::{BuildMode, detect_build_mode};

#[test]
fn detects_release_before_debug_when_both_binaries_exist() {
    let mode = detect_build_mode(
        Path::new("/repo/target/release/tjxy-server"),
        Path::new("/repo/target/debug/tjxy-server"),
        true,
        true,
    );

    assert_eq!(mode, BuildMode::Release);
}

#[test]
fn detects_no_build_when_neither_binary_exists() {
    assert_eq!(
        detect_build_mode(
            Path::new("/repo/target/release/tjxy-server"),
            Path::new("/repo/target/debug/tjxy-server"),
            false,
            false,
        ),
        BuildMode::None
    );
}
