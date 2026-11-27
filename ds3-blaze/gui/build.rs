//! Sets `DS3_BLAZE_COMMIT`, `DS3_BLAZE_COMMIT_SHORT` and
//! `DS3_BLAZE_COMMIT_DATE` to the full hash, the short hash and the date of the
//! commit being built, or each to nothing outside a git checkout.

use std::path::Path;
use std::process::Command;

fn main() {
    let log = git(&["log", "-1", "--format=%H %h %cs"]).unwrap_or_default();
    let mut fields = log.split(' ');
    for name in ["COMMIT", "COMMIT_SHORT", "COMMIT_DATE"] {
        let value = fields.next().unwrap_or_default();
        println!("cargo:rustc-env=DS3_BLAZE_{name}={value}");
    }
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
