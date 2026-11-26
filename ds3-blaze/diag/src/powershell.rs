//! Windows PowerShell scripts run from a temporary file.

use std::io;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Number that keeps the temporary files of concurrent runs apart.
static RUNS: AtomicU32 = AtomicU32::new(0);

/// Runs `script` with the named arguments `args` and returns what it printed.
///
/// The script is written to a file in the temporary folder and run with
/// `-File`, which takes it whatever the execution policy and the console code
/// page; the file is removed afterwards. Each argument is passed as
/// `-<name> <value>`.
///
/// # Errors
///
/// Returns the error of writing the file or starting PowerShell, or one with
/// the script's error output when it exits with a failure status.
pub fn run(script: &str, args: &[(&str, String)]) -> io::Result<String> {
    let path = std::env::temp_dir().join(format!(
        "ds3-blaze-{}-{}.ps1",
        std::process::id(),
        RUNS.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, script)?;
    let mut command = Command::new("powershell");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&path);
    for (name, value) in args {
        command.arg(format!("-{name}")).arg(value);
    }
    let output = command.output();
    let _ = std::fs::remove_file(&path);
    let output = output?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        Err(io::Error::other(format!(
            "PowerShell failed with {}: {}",
            output.status,
            error.trim()
        )))
    }
}
