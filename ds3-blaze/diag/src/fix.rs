//! Changes to the Windows network settings that resolve a finding of the
//! network check; each needs administrator rights.

use crate::netcheck::Fix;
use crate::p2p::GAME_PORT;
use crate::powershell::{self, ACCESS_DENIED};
use std::io;

/// Returns the display name of the firewall rule `fix` adds, or `None` when it
/// adds none.
///
/// Every name reads `ds3-blaze: allow <what the rule opens> (<what for>)`, so
/// the rules sort together in the firewall's list: the game's port for the
/// game rule, the program's file name for the server rule.
pub fn rule_name(fix: &Fix) -> Option<String> {
    let (opens, purpose) = match fix {
        Fix::GameRule => (format!("UDP {GAME_PORT}"), "game P2P"),
        Fix::ServerRule(program) => {
            let file = program.file_name().unwrap_or(program.as_os_str());
            (file.to_string_lossy().into_owned(), "server")
        }
        Fix::PrivateProfile(_) => return None,
    };
    Some(format!("ds3-blaze: allow {opens} ({purpose})"))
}

/// Returns the PowerShell script that carries out `fix`.
///
/// The script exits with [`ACCESS_DENIED`] without administrator rights and
/// stops at the first error. A rule is added after removing any rule of the
/// same name, so running the script twice leaves one rule.
pub fn script(fix: &Fix) -> String {
    let name = rule_name(fix).unwrap_or_default();
    let change = match fix {
        // The game's own path differs between installs, so the rule opens the
        // port rather than naming the executable.
        Fix::GameRule => replace_rule(&name, &format!("-Protocol UDP -LocalPort {GAME_PORT}")),
        Fix::ServerRule(program) => replace_rule(
            &name,
            &format!("-Program {}", quoted(&program.display().to_string())),
        ),
        Fix::PrivateProfile(address) => format!(
            "$index = (Get-NetIPAddress -IPAddress {} -AddressFamily IPv4).InterfaceIndex\n\
             Set-NetConnectionProfile -InterfaceIndex $index -NetworkCategory Private\n",
            quoted(&address.to_string())
        ),
    };
    format!(
        "$ErrorActionPreference = 'Stop'\n\
         $identity = [Security.Principal.WindowsIdentity]::GetCurrent()\n\
         $principal = New-Object Security.Principal.WindowsPrincipal $identity\n\
         if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {{ exit {ACCESS_DENIED} }}\n\
         {change}"
    )
}

/// Carries out `fix`.
///
/// # Errors
///
/// Returns [`io::ErrorKind::PermissionDenied`] without administrator rights,
/// or the error PowerShell reports.
pub fn apply(fix: &Fix) -> io::Result<()> {
    powershell::run(&script(fix), &[]).map(drop)
}

/// Returns the lines that replace the inbound allow rule `name` with one
/// limited by `filter`, in every firewall profile.
fn replace_rule(name: &str, filter: &str) -> String {
    let name = quoted(name);
    format!(
        "Remove-NetFirewallRule -DisplayName {name} -ErrorAction SilentlyContinue\n\
         New-NetFirewallRule -DisplayName {name} -Direction Inbound -Action Allow -Profile Any {filter} | Out-Null\n"
    )
}

/// Returns `text` as a single-quoted PowerShell string.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::path::PathBuf;

    #[test]
    fn rule_names_share_one_pattern() {
        assert_eq!(
            rule_name(&Fix::GameRule),
            Some(format!("ds3-blaze: allow UDP {GAME_PORT} (game P2P)"))
        );
        assert_eq!(
            rule_name(&Fix::ServerRule(PathBuf::from(r"C:\Games\ds3-blaze.exe"))),
            Some("ds3-blaze: allow ds3-blaze.exe (server)".into())
        );
        assert_eq!(rule_name(&Fix::PrivateProfile(Ipv4Addr::LOCALHOST)), None);
    }

    #[test]
    fn game_rule_opens_the_game_port() {
        let script = script(&Fix::GameRule);
        let name = rule_name(&Fix::GameRule).expect("a rule");
        assert!(script.contains(&format!("Remove-NetFirewallRule -DisplayName '{name}'")));
        assert!(script.contains(&format!("New-NetFirewallRule -DisplayName '{name}'")));
        assert!(script.contains(&format!("-Protocol UDP -LocalPort {GAME_PORT}")));
        assert!(script.contains("-Direction Inbound -Action Allow -Profile Any"));
    }

    #[test]
    fn server_rule_quotes_the_program_path() {
        let script = script(&Fix::ServerRule(PathBuf::from(
            r"C:\Games\O'Brien's\ds3-blaze.exe",
        )));
        assert!(script.contains(r"-Program 'C:\Games\O''Brien''s\ds3-blaze.exe'"));
        assert!(script.contains("-DisplayName 'ds3-blaze: allow ds3-blaze.exe (server)'"));
    }

    #[test]
    fn private_profile_changes_the_adapter_holding_the_address() {
        let script = script(&Fix::PrivateProfile(Ipv4Addr::new(26, 0, 0, 1)));
        assert!(script.contains("Get-NetIPAddress -IPAddress '26.0.0.1' -AddressFamily IPv4"));
        assert!(script.contains("-NetworkCategory Private"));
    }

    #[test]
    fn every_script_stops_without_administrator_rights() {
        for fix in [
            Fix::GameRule,
            Fix::ServerRule(PathBuf::from("ds3-blaze.exe")),
            Fix::PrivateProfile(Ipv4Addr::LOCALHOST),
        ] {
            let script = script(&fix);
            assert!(script.starts_with("$ErrorActionPreference = 'Stop'\n"));
            assert!(script.contains(&format!("{{ exit {ACCESS_DENIED} }}")));
        }
    }
}
