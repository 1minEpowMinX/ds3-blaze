//! Timestamped log lines written to stdout and to a session file.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

static FILE: Mutex<Option<File>> = Mutex::new(None);

/// Opens `logs/ds3-blaze-<timestamp>.log` next to the current directory and
/// returns its path; later [`write`] calls append to it.
pub fn init() -> std::io::Result<PathBuf> {
    fs::create_dir_all("logs")?;
    let name = chrono::Local::now()
        .format("ds3-blaze-%Y%m%d-%H%M%S.log")
        .to_string();
    let path = PathBuf::from("logs").join(name);
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    *FILE.lock().expect("log mutex") = Some(file);
    Ok(path)
}

/// Writes `message` prefixed with local time and `tag` to stdout and, after
/// [`init`], to the log file.
pub fn write(tag: &str, message: &str) {
    let line = format!(
        "{} [{tag}] {message}",
        chrono::Local::now().format("%H:%M:%S%.3f")
    );
    println!("{line}");
    if let Some(file) = FILE.lock().expect("log mutex").as_mut() {
        let _ = writeln!(file, "{line}");
    }
}

/// Formats its arguments and passes them to [`write`] under the given tag.
#[macro_export]
macro_rules! log {
    ($tag:expr, $($arg:tt)*) => {
        $crate::log::write($tag, &format!($($arg)*))
    };
}
