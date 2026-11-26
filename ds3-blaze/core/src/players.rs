//! Names and admission of the players a server knows, changeable while it runs.

use crate::event;
use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv4Addr;
use std::ops::RangeInclusive;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lengths a nickname may have; EA persona names have the same.
pub const NICKNAME_LENGTH: RangeInclusive<usize> = 4..=16;

/// Returns `Ok` when `name` has a length within [`NICKNAME_LENGTH`] and only
/// ASCII letters, digits, `-` and `_`, or the reason it does not.
pub fn check_nickname(name: &str) -> Result<(), String> {
    if let Some(c) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_'))
    {
        return Err(format!(
            "{c:?} is not allowed; use letters, digits, - and _"
        ));
    }
    // The characters are checked first, so the byte length here is the character count.
    if !NICKNAME_LENGTH.contains(&name.len()) {
        return Err(format!(
            "a name has {} to {} characters",
            NICKNAME_LENGTH.start(),
            NICKNAME_LENGTH.end()
        ));
    }
    Ok(())
}

/// Contents of a [`PlayerBook`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roster {
    /// Display name of a player connecting from this machine or from the
    /// advertised address.
    pub own_name: String,
    /// Display names by address; an entry wins over `own_name`.
    pub names: BTreeMap<Ipv4Addr, String>,
    /// Addresses admitted while `only_allowed` is on.
    pub allowed: BTreeSet<Ipv4Addr>,
    /// Whether Blaze accepts connections only from this machine, the advertised
    /// address and `allowed`.
    pub only_allowed: bool,
}

/// Player names and admission rules shared by every session of a server.
///
/// A change applies to the connections and logins that follow it; a player
/// already logged in keeps the name it logged in with.
pub struct PlayerBook {
    advertise: Ipv4Addr,
    roster: RwLock<Roster>,
}

impl PlayerBook {
    /// Returns a book holding `roster` for a server advertised at `advertise`.
    pub fn new(advertise: Ipv4Addr, roster: Roster) -> Self {
        Self {
            advertise,
            roster: RwLock::new(roster),
        }
    }

    /// Returns a copy of the book's contents.
    pub fn roster(&self) -> Roster {
        self.read().clone()
    }

    /// Returns whether a connection from `ip` may open a session: always while
    /// `only_allowed` is off, otherwise only from this machine, the advertised
    /// address or an allowed address.
    pub fn admits(&self, ip: Ipv4Addr) -> bool {
        let roster = self.read();
        !roster.only_allowed
            || ip.is_loopback()
            || ip == self.advertise
            || roster.allowed.contains(&ip)
    }

    /// Returns the display name of a player connecting from `ip`.
    ///
    /// An address with an entry in `names` gets that entry; this machine and the
    /// advertised address get `own_name`; any other address gets `Player-`
    /// followed by its last octet.
    pub fn name_for(&self, ip: Ipv4Addr) -> String {
        let roster = self.read();
        if let Some(name) = roster.names.get(&ip) {
            return name.clone();
        }
        if ip.is_loopback() || ip == self.advertise {
            return roster.own_name.clone();
        }
        format!("Player-{}", ip.octets()[3])
    }

    /// Sets the display name of `ip` to `name`, or removes its entry for `None`.
    pub fn set_name(&self, ip: Ipv4Addr, name: Option<String>) {
        match name {
            Some(name) => self.write().names.insert(ip, name),
            None => self.write().names.remove(&ip),
        };
        event::changed();
    }

    /// Adds `ip` to the allowed addresses when `allowed` is true, removes it otherwise.
    pub fn set_allowed(&self, ip: Ipv4Addr, allowed: bool) {
        if allowed {
            self.write().allowed.insert(ip);
        } else {
            self.write().allowed.remove(&ip);
        }
        event::changed();
    }

    /// Turns the "only allowed players" switch on or off.
    pub fn set_only_allowed(&self, on: bool) {
        self.write().only_allowed = on;
        event::changed();
    }

    fn read(&self) -> RwLockReadGuard<'_, Roster> {
        // The roster holds plain data, so one left by a panicking writer is still usable.
        self.roster
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> RwLockWriteGuard<'_, Roster> {
        self.roster
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADVERTISE: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);
    const GUEST: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 2);
    const STRANGER: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 3);

    fn book(only_allowed: bool) -> PlayerBook {
        PlayerBook::new(
            ADVERTISE,
            Roster {
                own_name: "host".into(),
                names: BTreeMap::from([(GUEST, "guest".into())]),
                allowed: BTreeSet::from([GUEST]),
                only_allowed,
            },
        )
    }

    #[test]
    fn only_allowed_admits_this_machine_and_allowed_players() {
        let book = book(true);
        assert!(book.admits(Ipv4Addr::LOCALHOST));
        assert!(book.admits(ADVERTISE));
        assert!(book.admits(GUEST));
        assert!(!book.admits(STRANGER));
    }

    #[test]
    fn without_only_allowed_every_address_is_admitted() {
        assert!(book(false).admits(STRANGER));
    }

    #[test]
    fn names_come_from_the_entry_then_own_name_then_the_last_octet() {
        let book = book(false);
        assert_eq!(book.name_for(GUEST), "guest");
        assert_eq!(book.name_for(Ipv4Addr::LOCALHOST), "host");
        assert_eq!(book.name_for(ADVERTISE), "host");
        assert_eq!(book.name_for(STRANGER), "Player-3");
        book.set_name(ADVERTISE, Some("renamed host".into()));
        assert_eq!(book.name_for(ADVERTISE), "renamed host");
        assert_eq!(book.name_for(Ipv4Addr::LOCALHOST), "host");
    }

    #[test]
    fn changes_apply_to_the_next_check() {
        let book = book(true);
        book.set_allowed(STRANGER, true);
        assert!(book.admits(STRANGER));
        book.set_allowed(STRANGER, false);
        assert!(!book.admits(STRANGER));
        book.set_only_allowed(false);
        assert!(book.admits(STRANGER));

        book.set_name(STRANGER, Some("Carver".into()));
        assert_eq!(book.name_for(STRANGER), "Carver");
        book.set_name(STRANGER, None);
        assert_eq!(book.name_for(STRANGER), "Player-3");
    }

    #[test]
    fn nicknames_follow_ea_persona_names() {
        let shortest = "a".repeat(*NICKNAME_LENGTH.start());
        let longest = "a".repeat(*NICKNAME_LENGTH.end());
        assert_eq!(check_nickname(&shortest), Ok(()));
        assert_eq!(check_nickname(&longest), Ok(()));
        assert!(check_nickname(&shortest[1..]).is_err());
        assert!(check_nickname(&format!("{longest}a")).is_err());
        assert_eq!(check_nickname("Ellie_Langford-1"), Ok(()));
        for refused in ["Айзек", "Isaac Clarke", "Isaac\nClarke", ""] {
            assert!(check_nickname(refused).is_err(), "{refused:?}");
        }
    }

    #[test]
    fn roster_returns_the_current_contents() {
        let book = book(true);
        book.set_name(STRANGER, Some("Carver".into()));
        book.set_only_allowed(false);
        let roster = book.roster();
        assert_eq!(
            roster.names.get(&STRANGER).map(String::as_str),
            Some("Carver")
        );
        assert!(!roster.only_allowed);
    }
}
