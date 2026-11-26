//! Changes to the Windows network settings that resolve a finding of the
//! network check; each needs administrator rights.

use crate::netcheck::Fix;
use crate::p2p::GAME_PORT;
use crate::powershell::{self, ACCESS_DENIED};
use std::io;

/// Display name of the rule [`Fix::GameRule`] adds.
pub const GAME_RULE_NAME: &str = "ds3-blaze: Dead Space 3 P2P";
/// Display name of the rule [`Fix::ServerRule`] adds.
pub const SERVER_RULE_NAME: &str = "ds3-blaze: server";

/// Returns the PowerShell script that carries out `fix`.
///
/// The script exits with [`ACCESS_DENIED`] without administrator rights and
/// stops at the first error. A rule is added after removing any rule of the
/// same name, so running the script twice leaves one rule.
pub fn script(fix: &Fix) -> String {
    let change = match fix {
        // The game's own path differs between installs, so the rule opens the
        // port rather than naming the executable.
        Fix::GameRule => replace_rule(
            GAME_RULE_NAME,
            &format!("-Protocol UDP -LocalPort {GAME_PORT}"),
        ),
        Fix::ServerRule(program) => replace_rule(
            SERVER_RULE_NAME,
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
    fn game_rule_opens_the_game_port() {
        let script = script(&Fix::GameRule);
        assert!(script.contains(&format!(
            "Remove-NetFirewallRule -DisplayName '{GAME_RULE_NAME}'"
        )));
        assert!(script.contains(&format!("-Protocol UDP -LocalPort {GAME_PORT}")));
        assert!(script.contains("-Direction Inbound -Action Allow -Profile Any"));
    }

    #[test]
    fn server_rule_quotes_the_program_path() {
        let script = script(&Fix::ServerRule(PathBuf::from(
            r"C:\Games\O'Brien's\ds3-blaze.exe",
        )));
        assert!(script.contains(r"-Program 'C:\Games\O''Brien''s\ds3-blaze.exe'"));
        assert!(script.contains(&format!("-DisplayName '{SERVER_RULE_NAME}'")));
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
