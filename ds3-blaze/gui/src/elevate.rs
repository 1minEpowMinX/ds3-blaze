//! Running the command line elevated, one administrator prompt per run.
//!
//! The window itself runs without administrator rights, so the server it
//! starts never runs elevated either.

use ds3_blaze_diag::netcheck::Fix;
use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// File name of the command line's executable, beside the window's.
pub const CLI_EXE: &str = "ds3-blaze-cli.exe";

/// Returns the path of [`CLI_EXE`] beside the running executable.
pub fn cli_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(CLI_EXE)))
        .unwrap_or_else(|| PathBuf::from(CLI_EXE))
}

/// Returns the command line's arguments for `fix`.
pub fn fix_args(fix: &Fix) -> Vec<String> {
    let mut args = vec!["fix".to_string()];
    match fix {
        Fix::GameRule => args.push("game-rule".into()),
        Fix::ServerRule(program) => args.extend([
            "server-rule".into(),
            "--program".into(),
            program.display().to_string(),
        ]),
        Fix::PrivateProfile(address) => args.extend([
            "network-profile".into(),
            "--address".into(),
            address.to_string(),
        ]),
    }
    args
}

/// Returns the command line's arguments for applying `fixes` in one run:
/// [`fix_args`] for one fix, `fix several` with one flag per fix for more.
pub fn fixes_args(fixes: &[Fix]) -> Vec<String> {
    if let [fix] = fixes {
        return fix_args(fix);
    }
    let mut args = vec!["fix".to_string(), "several".to_string()];
    for fix in fixes {
        match fix {
            Fix::GameRule => args.push("--game-rule".into()),
            Fix::ServerRule(program) => {
                args.extend(["--server-rule".into(), program.display().to_string()])
            }
            Fix::PrivateProfile(address) => {
                args.extend(["--network-profile".into(), address.to_string()])
            }
        }
    }
    args
}

/// Returns the command line's arguments for pointing the game at `address`
/// through the hosts file, or back at the EA servers for `None`.
pub fn hosts_args(address: Option<std::net::Ipv4Addr>) -> Vec<String> {
    match address {
        Some(address) => vec![
            "hosts".into(),
            "enable".into(),
            "--address".into(),
            address.to_string(),
        ],
        None => vec!["hosts".into(), "disable".into()],
    }
}

/// Returns `args` joined into one command line that `CommandLineToArgvW`
/// splits back into the same arguments.
pub fn command_line(args: &[String]) -> String {
    args.iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns `arg` as `CommandLineToArgvW` reads it back: unchanged when it is
/// non-empty and holds no white space or quote, quoted otherwise.
fn quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '"']) {
        return arg.to_string();
    }
    let mut quoted = String::from("\"");
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            // Backslashes before a quote are doubled, and the quote escaped.
            '"' => {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.push_str(&"\\".repeat(backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    // Backslashes before the closing quote are doubled so that it stays one.
    quoted.push_str(&"\\".repeat(backslashes * 2));
    quoted.push('"');
    quoted
}

/// Returns `text` as a NUL-terminated UTF-16 string.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain([0]).collect()
}

/// Runs [`cli_path`] with `args` elevated and hidden, after the administrator
/// prompt, and returns its exit code.
///
/// # Errors
///
/// Returns an error saying the prompt was declined when the player declines
/// it, and the system's error when the program cannot be started.
pub async fn run_cli(args: Vec<String>) -> io::Result<i32> {
    tokio::task::spawn_blocking(move || run_blocking(&args))
        .await
        .map_err(io::Error::other)?
}

/// Runs [`cli_path`] as [`run_cli`] describes, waiting on this thread.
fn run_blocking(args: &[String]) -> io::Result<i32> {
    let verb = wide(OsStr::new("runas"));
    let file = wide(cli_path().as_os_str());
    let parameters = wide(OsStr::new(&command_line(args)));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, and the strings it points
    // to live until the call returns.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // SAFETY: reads the calling thread's last error, set by the call above.
        let error = unsafe { GetLastError() };
        if error == ERROR_CANCELLED {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the administrator prompt was declined",
            ));
        }
        return Err(io::Error::from_raw_os_error(error as i32));
    }
    let process = Process(info.hProcess);
    let mut code = 0u32;
    // SAFETY: `process` holds the handle SEE_MASK_NOCLOSEPROCESS returned,
    // open until `process` is dropped.
    unsafe {
        WaitForSingleObject(process.0, INFINITE);
        if GetExitCodeProcess(process.0, &mut code) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(code as i32)
}

/// A process handle, closed when dropped.
struct Process(HANDLE);

impl Drop for Process {
    /// Closes the handle.
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle is open and owned by this value alone.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(args: &[&str]) -> String {
        command_line(&args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn the_command_line_is_built_under_this_name() {
        let manifest: toml::Table =
            toml::from_str(include_str!("../../cli/Cargo.toml")).expect("the manifest parses");
        let bin = &manifest["bin"].as_array().expect("a [[bin]] table")[0];
        assert_eq!(
            format!("{}.exe", bin["name"].as_str().expect("a name")),
            CLI_EXE
        );
    }

    #[test]
    fn one_fix_runs_as_its_own_command() {
        assert_eq!(fixes_args(&[Fix::GameRule]), fix_args(&Fix::GameRule));
    }

    #[test]
    fn several_fixes_run_as_fix_several() {
        let fixes = [
            Fix::GameRule,
            Fix::ServerRule(r"C:\My Games\ds3-blaze.exe".into()),
            Fix::PrivateProfile(std::net::Ipv4Addr::new(26, 0, 0, 2)),
        ];
        assert_eq!(
            fixes_args(&fixes),
            [
                "fix",
                "several",
                "--game-rule",
                "--server-rule",
                r"C:\My Games\ds3-blaze.exe",
                "--network-profile",
                "26.0.0.2"
            ]
        );
    }

    #[test]
    fn plain_arguments_stay_as_they_are() {
        assert_eq!(
            line(&["hosts", "enable", "--address", "26.0.0.1"]),
            "hosts enable --address 26.0.0.1"
        );
    }

    #[test]
    fn white_space_and_empty_arguments_are_quoted() {
        assert_eq!(line(&["C:\\My Games\\x.exe"]), "\"C:\\My Games\\x.exe\"");
        assert_eq!(line(&["a", ""]), "a \"\"");
    }

    #[test]
    fn quotes_and_trailing_backslashes_are_escaped() {
        assert_eq!(line(&["say \"hi\""]), "\"say \\\"hi\\\"\"");
        assert_eq!(line(&["C:\\My Games\\"]), "\"C:\\My Games\\\\\"");
        // A backslash before a quote is doubled, then the quote escaped.
        assert_eq!(line(&["a\\\"b"]), "\"a\\\\\\\"b\"");
    }
}
