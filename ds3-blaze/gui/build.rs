//! Sets `DS3_BLAZE_COMMIT` to the short hash of the commit being built, or to
//! nothing outside a git checkout.

use std::path::Path;
use std::process::Command;

fn main() {
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    println!("cargo:rustc-env=DS3_BLAZE_COMMIT={commit}");
    // A commit moves the branch HEAD names, and a checkout changes HEAD itself;
    // a branch packed into packed-refs has no file of its own.
    let mut watched = vec![git(&["rev-parse", "--git-path", "HEAD"])];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        watched.push(git(&["rev-parse", "--git-path", &branch]));
        watched.push(git(&["rev-parse", "--git-path", "packed-refs"]));
    }
    for path in watched.into_iter().flatten() {
        if Path::new(&path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}

/// Returns what `git` prints for `args`, `None` when it fails or is missing.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
