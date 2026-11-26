//! The Windows hosts file section that sends the game's redirector lookups to a
//! ds3-blaze server.
//!
//! The entries sit at the end of the file between [`BEGIN_LINE`] and
//! [`END_LINE`], as Docker Desktop and similar programs mark theirs. Removing
//! them also removes any line that ends with [`LEGACY_MARKER`] and leaves the
//! rest of the file untouched, in its own encoding and line endings.

use std::fmt;
use std::fs;
use std::io;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Line that opens the section.
pub const BEGIN_LINE: &str = "# Added by ds3-blaze";
/// Line that closes the section.
// Unlike Docker Desktop's "# End of section", this line names its owner, so
// removing the section never reaches into another program's.
pub const END_LINE: &str = "# End of ds3-blaze section";
/// Ending of the entries written one per line, without a section.
pub const LEGACY_MARKER: &str = "# ds3-blaze";
/// Host names the section maps to the server.
pub const REDIRECTED: [&str; 1] = ["gosredirector.ea.com"];

/// Name of the backup written next to the hosts file.
const BACKUP_NAME: &str = "hosts.ds3-blaze.bak";

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const UTF16_LE_BOM: &[u8] = b"\xFF\xFE";
const UTF16_BE_BOM: &[u8] = b"\xFE\xFF";

/// Change made to the hosts file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Maps [`REDIRECTED`] to the address, replacing earlier ds3-blaze entries.
    Enable(Ipv4Addr),
    /// Removes the ds3-blaze entries.
    Disable,
}

/// Where the hosts file sends the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No ds3-blaze entry: the game reaches the EA servers.
    EaServers,
    /// A ds3-blaze entry maps the redirector to this address.
    Redirected(Ipv4Addr),
    /// The file cannot be read or changed safely.
    Broken(Problem),
}

/// Reason the hosts file cannot be changed safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The section opens without [`END_LINE`], so its extent is unknown.
    Unterminated,
    /// The file starts with a byte order mark that its contents do not match.
    Undecodable,
}

impl fmt::Display for Problem {
    /// Writes what is wrong and what the player can do about it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unterminated => write!(
                f,
                "the hosts file has '{BEGIN_LINE}' without '{END_LINE}'; \
                 remove the ds3-blaze lines from it by hand"
            ),
            Self::Undecodable => write!(
                f,
                "the hosts file starts with a byte order mark that its text does not match"
            ),
        }
    }
}

impl std::error::Error for Problem {}

/// Contents of the hosts file and its backup after a change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewritten {
    /// The file with the change applied.
    pub updated: Vec<u8>,
    /// The file without any ds3-blaze entry.
    pub backup: Vec<u8>,
}

/// Text encoding of a hosts file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Encoding {
    /// No byte order mark: one byte per character.
    Ansi,
    Utf8,
    Utf16Le,
    Utf16Be,
}

/// Returns `file` with `action` applied, and the backup to keep beside it.
///
/// The backup and the updated file keep the encoding and the line break of
/// `file`. The updated file ends with a line break when `file` did or when a
/// section was added.
///
/// # Errors
///
/// Returns the [`Problem`] that makes `file` unsafe to change.
pub fn rewrite(file: &[u8], action: Action) -> Result<Rewritten, Problem> {
    let (text, encoding) = decode(file)?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let (lines, final_break) = split_lines(&text);
    let (mut kept, _) = sort_out(&lines)?;
    // A backup taken with an entry in it would bring the redirect back when
    // restored, so it holds only the lines that stay.
    let backup = join_lines(&kept, newline, final_break);
    let added = match action {
        Action::Enable(address) => section(address),
        Action::Disable => Vec::new(),
    };
    kept.extend(added.iter().map(String::as_str));
    let updated = join_lines(&kept, newline, final_break || !added.is_empty());
    Ok(Rewritten {
        updated: encode(&updated, encoding),
        backup: encode(&backup, encoding),
    })
}

/// Returns where `file` sends the game.
///
/// The first ds3-blaze entry that maps a name in [`REDIRECTED`] to an IPv4
/// address gives the address; ds3-blaze lines without such an entry count as
/// none.
pub fn state(file: &[u8]) -> State {
    let found = decode(file).and_then(|(text, _)| {
        let (lines, _) = split_lines(&text);
        let (_, entries) = sort_out(&lines)?;
        Ok(entries.into_iter().find_map(entry_address))
    });
    match found {
        Ok(Some(address)) => State::Redirected(address),
        Ok(None) => State::EaServers,
        Err(problem) => State::Broken(problem),
    }
}

/// Returns the path of the system hosts file.
pub fn path() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join(r"System32\drivers\etc\hosts")
}

/// Returns the path of the backup [`apply`] writes beside the hosts file.
pub fn backup_path() -> PathBuf {
    path().with_file_name(BACKUP_NAME)
}

/// Reads the system hosts file and returns where it sends the game.
///
/// # Errors
///
/// Returns the error of reading the file.
pub fn status() -> io::Result<State> {
    Ok(state(&fs::read(path())?))
}

/// Applies `action` to the system hosts file after writing the backup.
///
/// Both writes need an elevated process; without one Windows refuses them with
/// [`io::ErrorKind::PermissionDenied`].
///
/// # Errors
///
/// Returns the error of reading or writing either file, or the [`Problem`]
/// that makes the hosts file unsafe to change as [`io::ErrorKind::InvalidData`].
pub fn apply(action: Action) -> io::Result<()> {
    let path = path();
    let rewritten = rewrite(&fs::read(&path)?, action)
        .map_err(|problem| io::Error::new(io::ErrorKind::InvalidData, problem))?;
    fs::write(backup_path(), &rewritten.backup)?;
    fs::write(&path, &rewritten.updated)
}

/// Clears the Windows DNS cache with `ipconfig /flushdns`, so the game resolves
/// the redirector again at once.
///
/// # Errors
///
/// Returns the error of starting `ipconfig`, or one naming its exit status when
/// it fails.
pub fn flush_dns() -> io::Result<()> {
    let status = Command::new("ipconfig")
        .arg("/flushdns")
        .stdout(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "ipconfig /flushdns failed with {status}"
        )))
    }
}

/// Returns the lines of the section that maps [`REDIRECTED`] to `address`.
fn section(address: Ipv4Addr) -> Vec<String> {
    let entries = REDIRECTED.iter().map(|name| format!("{address} {name}"));
    std::iter::once(BEGIN_LINE.to_string())
        .chain(entries)
        .chain(std::iter::once(END_LINE.to_string()))
        .collect()
}

/// Returns the address `line` maps a name in [`REDIRECTED`] to, if it does.
fn entry_address(line: &str) -> Option<Ipv4Addr> {
    let entry = line.split('#').next()?;
    let mut words = entry.split_whitespace();
    let address = words.next()?.parse().ok()?;
    words
        .any(|word| {
            REDIRECTED
                .iter()
                .any(|name| word.eq_ignore_ascii_case(name))
        })
        .then_some(address)
}

/// Splits `lines` into the lines that are not ds3-blaze's and the entries that
/// are, without the section's own markers.
///
/// # Errors
///
/// Returns [`Problem::Unterminated`] when a section has no end line.
fn sort_out<'a>(lines: &[&'a str]) -> Result<(Vec<&'a str>, Vec<&'a str>), Problem> {
    let mut kept = Vec::new();
    let mut entries = Vec::new();
    let mut in_section = false;
    for &line in lines {
        let trimmed = line.trim();
        if in_section {
            if trimmed == END_LINE {
                in_section = false;
            } else {
                entries.push(line);
            }
        } else if trimmed == BEGIN_LINE {
            in_section = true;
        } else if line.ends_with(LEGACY_MARKER) {
            entries.push(line);
        } else {
            kept.push(line);
        }
    }
    // Guessing where a section without an end line stops could delete the
    // player's own entries.
    if in_section {
        return Err(Problem::Unterminated);
    }
    Ok((kept, entries))
}

/// Returns the lines of `text` without their CRLF or LF breaks, and whether the
/// last one had a break.
fn split_lines(text: &str) -> (Vec<&str>, bool) {
    let mut lines = Vec::new();
    let mut final_break = false;
    for piece in text.split_inclusive('\n') {
        final_break = piece.ends_with('\n');
        let line = piece.strip_suffix('\n').unwrap_or(piece);
        lines.push(if final_break {
            line.strip_suffix('\r').unwrap_or(line)
        } else {
            line
        });
    }
    (lines, final_break)
}

/// Returns `lines` joined by `newline`, with a final `newline` when
/// `final_break` is set and there is a line.
fn join_lines(lines: &[&str], newline: &str, final_break: bool) -> String {
    let mut text = lines.join(newline);
    if final_break && !lines.is_empty() {
        text.push_str(newline);
    }
    text
}

/// Returns the text of `file` and the encoding to write it back in.
///
/// # Errors
///
/// Returns [`Problem::Undecodable`] when the contents do not match the byte
/// order mark.
fn decode(file: &[u8]) -> Result<(String, Encoding), Problem> {
    if let Some(rest) = file.strip_prefix(UTF8_BOM) {
        let text = std::str::from_utf8(rest).map_err(|_| Problem::Undecodable)?;
        return Ok((text.to_owned(), Encoding::Utf8));
    }
    for (bom, encoding) in [
        (UTF16_LE_BOM, Encoding::Utf16Le),
        (UTF16_BE_BOM, Encoding::Utf16Be),
    ] {
        if let Some(rest) = file.strip_prefix(bom) {
            let (pairs, odd_byte) = rest.as_chunks::<2>();
            if !odd_byte.is_empty() {
                return Err(Problem::Undecodable);
            }
            let units: Vec<u16> = pairs
                .iter()
                .map(|&pair| match encoding {
                    Encoding::Utf16Le => u16::from_le_bytes(pair),
                    _ => u16::from_be_bytes(pair),
                })
                .collect();
            let text = String::from_utf16(&units).map_err(|_| Problem::Undecodable)?;
            return Ok((text, encoding));
        }
    }
    // Each byte becomes the character of the same number (Latin-1), which
    // encodes back to that byte, so text in whatever ANSI code page Windows uses
    // passes through unchanged. The markers and the added lines are ASCII, which
    // every ANSI code page shares.
    Ok((
        file.iter().map(|&byte| char::from(byte)).collect(),
        Encoding::Ansi,
    ))
}

/// Returns `text` in `encoding`, with its byte order mark if it has one.
fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Ansi => text
            .chars()
            .map(|c| u8::try_from(c).expect("ANSI text holds only Latin-1 characters"))
            .collect(),
        Encoding::Utf8 => [UTF8_BOM, text.as_bytes()].concat(),
        Encoding::Utf16Le => UTF16_LE_BOM
            .iter()
            .copied()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect(),
        Encoding::Utf16Be => UTF16_BE_BOM
            .iter()
            .copied()
            .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST: Ipv4Addr = Ipv4Addr::new(26, 1, 2, 3);
    const OTHER_HOST: Ipv4Addr = Ipv4Addr::new(26, 4, 5, 6);

    fn enable(file: &[u8]) -> Rewritten {
        rewrite(file, Action::Enable(HOST)).expect("rewritable")
    }

    fn disable(file: &[u8]) -> Rewritten {
        rewrite(file, Action::Disable).expect("rewritable")
    }

    fn section_text(address: Ipv4Addr, newline: &str) -> String {
        format!("{BEGIN_LINE}{newline}{address} gosredirector.ea.com{newline}{END_LINE}{newline}")
    }

    #[test]
    fn enable_appends_the_section_and_disable_removes_it() {
        let original = b"127.0.0.1 localhost\r\n::1 localhost\r\n";
        let enabled = enable(original);
        let expected = [&original[..], section_text(HOST, "\r\n").as_bytes()].concat();
        assert_eq!(enabled.updated, expected);
        assert_eq!(enabled.backup, original);
        assert_eq!(state(&enabled.updated), State::Redirected(HOST));

        let disabled = disable(&enabled.updated);
        assert_eq!(disabled.updated, original);
        assert_eq!(disabled.backup, original);
        assert_eq!(state(&disabled.updated), State::EaServers);
    }

    #[test]
    fn lf_line_breaks_are_kept() {
        let enabled = enable(b"127.0.0.1 localhost\n");
        assert_eq!(
            enabled.updated,
            format!("127.0.0.1 localhost\n{}", section_text(HOST, "\n")).into_bytes()
        );
    }

    #[test]
    fn empty_file_gets_only_the_section() {
        let enabled = enable(b"");
        assert_eq!(enabled.updated, section_text(HOST, "\n").into_bytes());
        assert_eq!(enabled.backup, b"");
        assert_eq!(disable(&enabled.updated).updated, b"");
    }

    #[test]
    fn file_without_a_final_line_break_keeps_its_last_line() {
        let original = b"127.0.0.1 localhost";
        let enabled = enable(original);
        assert_eq!(
            enabled.updated,
            format!("127.0.0.1 localhost\n{}", section_text(HOST, "\n")).into_bytes()
        );
        assert_eq!(enabled.backup, original);
        assert_eq!(disable(original).updated, original);
    }

    #[test]
    fn repeated_enable_leaves_one_section_with_the_newest_address() {
        let first = enable(b"127.0.0.1 localhost\n");
        let second = rewrite(&first.updated, Action::Enable(OTHER_HOST)).expect("rewritable");
        assert_eq!(
            second.updated,
            format!("127.0.0.1 localhost\n{}", section_text(OTHER_HOST, "\n")).into_bytes()
        );
        assert_eq!(second.backup, b"127.0.0.1 localhost\n");
        assert_eq!(state(&second.updated), State::Redirected(OTHER_HOST));
    }

    #[test]
    fn legacy_marker_lines_are_entries() {
        let original = b"127.0.0.1 localhost\n26.1.2.3 gosredirector.ea.com # ds3-blaze\n";
        assert_eq!(state(original), State::Redirected(HOST));
        let disabled = disable(original);
        assert_eq!(disabled.updated, b"127.0.0.1 localhost\n");
        assert_eq!(disabled.backup, b"127.0.0.1 localhost\n");
    }

    #[test]
    fn section_without_an_end_line_is_left_alone() {
        let original =
            format!("127.0.0.1 localhost\n{BEGIN_LINE}\n26.1.2.3 gosredirector.ea.com\n");
        assert_eq!(
            rewrite(original.as_bytes(), Action::Disable),
            Err(Problem::Unterminated)
        );
        assert_eq!(
            rewrite(original.as_bytes(), Action::Enable(HOST)),
            Err(Problem::Unterminated)
        );
        assert_eq!(
            state(original.as_bytes()),
            State::Broken(Problem::Unterminated)
        );
    }

    #[test]
    fn another_programs_section_stays() {
        let original =
            "# Added by Docker Desktop\n10.0.0.5 host.docker.internal\n# End of section\n";
        let enabled = enable(original.as_bytes());
        let disabled = disable(&enabled.updated);
        assert_eq!(disabled.updated, original.as_bytes());
        assert_eq!(state(original.as_bytes()), State::EaServers);
    }

    #[test]
    fn ansi_text_passes_through_byte_for_byte() {
        // "# Café" in Windows-1252 and two Shift-JIS bytes: neither is UTF-8.
        let original = b"# Caf\xE9\r\n# \x93\xFA\r\n127.0.0.1 localhost\r\n";
        let enabled = enable(original);
        assert!(enabled.updated.starts_with(original));
        assert_eq!(enabled.backup, original);
        assert_eq!(disable(&enabled.updated).updated, original);
    }

    #[test]
    fn utf8_byte_order_mark_is_kept() {
        let original = "\u{FEFF}# Café\r\n127.0.0.1 localhost\r\n".as_bytes();
        let enabled = enable(original);
        assert_eq!(
            enabled.updated,
            [original, section_text(HOST, "\r\n").as_bytes()].concat()
        );
        assert_eq!(enabled.backup, original);
        assert_eq!(state(&enabled.updated), State::Redirected(HOST));
    }

    #[test]
    fn utf16_byte_order_marks_are_kept() {
        let text = "# Café\r\n127.0.0.1 localhost\r\n";
        for encoding in [Encoding::Utf16Le, Encoding::Utf16Be] {
            let original = encode(text, encoding);
            let enabled = enable(&original);
            let expected = format!("{text}{}", section_text(HOST, "\r\n"));
            assert_eq!(enabled.updated, encode(&expected, encoding), "{encoding:?}");
            assert_eq!(enabled.backup, original, "{encoding:?}");
            assert_eq!(state(&enabled.updated), State::Redirected(HOST));
        }
    }

    #[test]
    fn text_that_does_not_match_its_byte_order_mark_is_refused() {
        let utf8 = b"\xEF\xBB\xBF# Caf\xE9\n";
        assert_eq!(rewrite(utf8, Action::Disable), Err(Problem::Undecodable));
        let odd_utf16 = b"\xFF\xFE#\x00 ";
        assert_eq!(state(odd_utf16), State::Broken(Problem::Undecodable));
    }

    #[test]
    fn commented_and_foreign_entries_are_not_redirects() {
        assert_eq!(
            state(b"# 26.1.2.3 gosredirector.ea.com\n"),
            State::EaServers
        );
        assert_eq!(state(b"26.1.2.3 gosredirector.ea.com\n"), State::EaServers);
        let empty_section = format!("{BEGIN_LINE}\n{END_LINE}\n");
        assert_eq!(state(empty_section.as_bytes()), State::EaServers);
    }
}
