//! Embed the git hash into `jev --version` for non-tagged builds, so a
//! locally built binary is identifiable. Tagged builds stay clean. A local
//! estimate failing must never break the build: any git failure falls back
//! to no stamp.
use std::process::Command;

fn main() {
    let hash = git_hash();
    println!("cargo:rustc-env=JEV_GIT_HASH={hash}");
    // Deliberately no `rerun-if-changed` line: `.git/HEAD` does not exist as a
    // file in linked worktrees (there `.git` is a file pointing at the shared
    // gitdir), so that directive would pin the stamp to a stale hash. With no
    // directive, every rebuild in this crate re-stamps; that is cheap and
    // correct.
}

/// Short hash of HEAD, or empty when the build cannot see git (tarball
/// builds) or the commit is exactly a tag.
fn git_hash() -> String {
    // Tag marks a release; no stamp there.
    if Command::new("git")
        .args(["describe", "--exact-match", "--tags"])
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return String::new();
    }
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
