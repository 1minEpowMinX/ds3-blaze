//! Network adapters of this PC that hold IPv4 addresses, read through the IP
//! Helper API, and which of them look like a VPN's.

use std::io;
use std::net::Ipv4Addr;
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, ERROR_SUCCESS};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

/// Lower-case fragments of adapter names and descriptions that mark a VPN or
/// virtual tunnel adapter.
const VPN_MARKERS: &[&str] = &[
    "vpn",
    "radmin",
    "hamachi",
    "zerotier",
    "tailscale",
    "wireguard",
    "wintun",
    "openvpn",
    "tap-",
    "tap ",
    "tun",
    "nordlynx",
    "softether",
    "anyconnect",
    "fortinet",
    "pangp",
    "outline",
    "amnezia",
    "proton",
];

/// Size of the first buffer handed to `GetAdaptersAddresses`, the one its
/// documentation recommends.
const FIRST_BUFFER_SIZE: u32 = 15 * 1024;

/// A network adapter with its IPv4 addresses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adapter {
    /// Name Windows shows for the connection, such as "Radmin VPN".
    pub name: String,
    pub description: String,
    pub ipv4: Vec<Ipv4Addr>,
    /// Whether the adapter's operational status is up.
    pub up: bool,
}

impl Adapter {
    /// Returns whether the name or the description looks like a VPN adapter's.
    pub fn is_vpn(&self) -> bool {
        is_vpn(&self.name, &self.description)
    }
}

/// Returns whether an adapter name or description holds one of the fragments
/// that mark a VPN adapter, ignoring case.
pub fn is_vpn(name: &str, description: &str) -> bool {
    let text = format!("{name} {description}").to_lowercase();
    VPN_MARKERS.iter().any(|marker| text.contains(marker))
}

/// Returns the adapters of this PC that have IPv4 enabled, in the system's
/// order, each with its unicast IPv4 addresses.
///
/// # Errors
///
/// Returns the error `GetAdaptersAddresses` reports.
pub fn adapters() -> io::Result<Vec<Adapter>> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut size = FIRST_BUFFER_SIZE;
    loop {
        // u64 elements give the 8-byte alignment the adapter records need.
        let mut buffer = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
        let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: `first` points to `size` writable bytes, and `size` is a
        // valid pointer to their count.
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET),
                flags,
                std::ptr::null(),
                first,
                &mut size,
            )
        };
        match status {
            // SAFETY: the call filled `buffer` with a list starting at `first`,
            // and `buffer` lives until the list is read.
            ERROR_SUCCESS => return Ok(unsafe { read_list(first) }),
            ERROR_NO_DATA => return Ok(Vec::new()),
            // `size` now holds the size the list needs; the list may grow
            // again before the next call, hence the loop.
            ERROR_BUFFER_OVERFLOW => continue,
            code => return Err(io::Error::from_raw_os_error(code as i32)),
        }
    }
}

/// Returns the adapters of the list starting at `next`.
///
/// # Safety
///
/// `next` is null or the first record of a list `GetAdaptersAddresses`
/// filled, whose buffer outlives the call.
unsafe fn read_list(mut next: *const IP_ADAPTER_ADDRESSES_LH) -> Vec<Adapter> {
    let mut adapters = Vec::new();
    // SAFETY, for every dereference below: the records, their address lists
    // and their strings all lie in the buffer the caller keeps alive.
    while let Some(adapter) = unsafe { next.as_ref() } {
        let mut ipv4 = Vec::new();
        let mut unicast = adapter.FirstUnicastAddress.cast_const();
        while let Some(address) = unsafe { unicast.as_ref() } {
            let sockaddr = address.Address.lpSockaddr;
            if !sockaddr.is_null() && unsafe { (*sockaddr).sa_family } == AF_INET {
                let sin = unsafe { &*sockaddr.cast::<SOCKADDR_IN>() };
                // The address is in network order, so its bytes in memory are
                // the octets.
                let octets = unsafe { sin.sin_addr.S_un.S_addr }.to_ne_bytes();
                ipv4.push(Ipv4Addr::from(octets));
            }
            unicast = address.Next;
        }
        adapters.push(Adapter {
            name: unsafe { wide(adapter.FriendlyName) },
            description: unsafe { wide(adapter.Description) },
            ipv4,
            up: adapter.OperStatus == IfOperStatusUp,
        });
        next = adapter.Next;
    }
    adapters
}

/// Returns the text of a NUL-terminated UTF-16 string, empty for null.
///
/// # Safety
///
/// `text` is null or points to a NUL-terminated UTF-16 string.
unsafe fn wide(text: *const u16) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: the string is NUL-terminated, so every index read lies before
    // or at the terminator.
    let length = (0..).take_while(|&i| unsafe { *text.add(i) } != 0).count();
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, length) })
}

/// Returns the adapters a player picks an address from: those that are up
/// and hold an IPv4 address other than loopback, the ones [`is_vpn`]
/// recognises first, each group in the order of `adapters`.
pub fn choices(adapters: Vec<Adapter>) -> Vec<Adapter> {
    let (vpn, other): (Vec<Adapter>, Vec<Adapter>) = adapters
        .into_iter()
        .filter(|adapter| adapter.up && adapter.ipv4.iter().any(|ip| !ip.is_loopback()))
        .partition(Adapter::is_vpn);
    vpn.into_iter().chain(other).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(name: &str, description: &str, ipv4: &[Ipv4Addr], up: bool) -> Adapter {
        Adapter {
            name: name.into(),
            description: description.into(),
            ipv4: ipv4.to_vec(),
            up,
        }
    }

    const LAN: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 10);
    const RADMIN: Ipv4Addr = Ipv4Addr::new(26, 0, 0, 1);

    #[test]
    fn vpn_adapters_are_recognised_by_name_or_description() {
        assert!(is_vpn("Radmin VPN", "Famatech Radmin VPN Ethernet Adapter"));
        assert!(is_vpn("Ethernet 3", "ZeroTier Virtual Port"));
        assert!(is_vpn("WG", "WireGuard Tunnel"));
        assert!(!is_vpn("Ethernet", "Realtek PCIe GbE Family Controller"));
        assert!(!is_vpn("Wi-Fi", "Intel(R) Wi-Fi 6 AX201 160MHz"));
    }

    #[test]
    fn choices_put_vpn_adapters_first_and_keep_the_order() {
        let ethernet = adapter("Ethernet", "Realtek", &[LAN], true);
        let wifi = adapter("Wi-Fi", "Intel", &[Ipv4Addr::new(10, 0, 0, 5)], true);
        let radmin = adapter("Radmin VPN", "Famatech", &[RADMIN], true);
        let zerotier = adapter(
            "ZeroTier One",
            "ZeroTier",
            &[Ipv4Addr::new(10, 147, 0, 2)],
            true,
        );
        let chosen = choices(vec![
            ethernet.clone(),
            radmin.clone(),
            wifi.clone(),
            zerotier.clone(),
        ]);
        assert_eq!(chosen, [radmin, zerotier, ethernet, wifi]);
    }

    #[test]
    fn choices_leave_out_adapters_that_are_down_or_lack_an_address() {
        let down = adapter("Radmin VPN", "Famatech", &[RADMIN], false);
        let without = adapter("Ethernet", "Realtek", &[], true);
        let loopback = adapter(
            "Loopback Pseudo-Interface 1",
            "",
            &[Ipv4Addr::LOCALHOST],
            true,
        );
        assert!(choices(vec![down, without, loopback]).is_empty());
    }

    #[test]
    fn this_pc_lists_its_adapters() {
        let adapters = adapters().expect("GetAdaptersAddresses succeeds");
        for adapter in choices(adapters) {
            assert!(adapter.up, "{adapter:?}");
            assert!(
                adapter.ipv4.iter().any(|ip| !ip.is_loopback()),
                "{adapter:?}"
            );
        }
    }
}
