//! Checks around a ds3-blaze server: the Windows network settings for co-op
//! over a VPN, the fixes for what they find, and the UDP exchange between two
//! players on the game's port.

pub mod fix;
pub mod netcheck;
pub mod p2pcheck;
mod powershell;
