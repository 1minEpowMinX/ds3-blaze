//! The window's settings file, kept beside the executable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};

/// File name of the settings, beside the running executable.
pub const FILE_NAME: &str = "ds3-blaze.toml";

/// Which side of a co-op game this PC takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Host,
    Guest,
}

/// What the window remembers between runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The player's nickname.
    pub name: String,
    /// The mode chosen last.
    pub mode: Option<Mode>,
    /// The host's VPN address a guest connected to last.
    pub host: Option<Ipv4Addr>,
    /// Names the host gave players, by address; names guests sent are not kept.
    pub players: BTreeMap<Ipv4Addr, String>,
}

impl Settings {
    /// Returns the settings stored at `path`, or the default ones when the
    /// file is missing or cannot be read as settings.
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the settings to `path`.
    ///
    /// # Errors
    ///
    /// Returns the error of writing the file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = toml::to_string(self).map_err(io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Returns the path of [`FILE_NAME`] beside the running executable, or in the
/// working directory when the executable's path is unknown.
pub fn path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(FILE_NAME)))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ds3-blaze-gui-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn saved_settings_load_again() {
        let settings = Settings {
            name: "Carver".into(),
            mode: Some(Mode::Guest),
            host: Some(Ipv4Addr::new(26, 0, 0, 1)),
            players: BTreeMap::from([(Ipv4Addr::new(26, 0, 0, 2), "Ellie".into())]),
        };
        let path = temporary("round-trip.toml");
        settings.save(&path).expect("the file is written");
        let loaded = Settings::load(&path);
        std::fs::remove_file(&path).ok();
        assert_eq!(loaded, settings);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let settings: Settings =
            toml::from_str("name = \"Carver\"\nfuture = 1\n").expect("the text parses");
        assert_eq!(settings.name, "Carver");
    }

    #[test]
    fn a_missing_file_gives_the_default() {
        assert_eq!(
            Settings::load(&temporary("missing.toml")),
            Settings::default()
        );
    }
}
