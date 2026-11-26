//! Checks around a ds3-blaze server: the Windows network settings for co-op
//! over a VPN, the fixes for what they find, the UDP exchange between two
//! players on the game's port, and a guest's way to the host's server.

pub mod fix;
pub mod netcheck;
pub mod p2p;
mod powershell;
pub mod reach;
