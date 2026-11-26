//! The modes' state and what the window shows of it, with no drawing: each
//! flow takes the player's clicks and the background work's results and
//! returns the work to start, and the ui modules only read what it holds.

pub mod banner;
pub mod checks;
pub mod closing;
pub mod guest;
pub mod host;
pub mod link;
pub mod network;
pub mod session;
pub mod start;
pub mod step;

#[cfg(test)]
mod tests {
    /// Every flow module's source, by file name.
    const SOURCES: [(&str, &str); 10] = [
        ("banner.rs", include_str!("banner.rs")),
        ("checks.rs", include_str!("checks.rs")),
        ("closing.rs", include_str!("closing.rs")),
        ("guest.rs", include_str!("guest.rs")),
        ("host.rs", include_str!("host.rs")),
        ("link.rs", include_str!("link.rs")),
        ("network.rs", include_str!("network.rs")),
        ("session.rs", include_str!("session.rs")),
        ("start.rs", include_str!("start.rs")),
        ("step.rs", include_str!("step.rs")),
    ];

    #[test]
    fn no_flow_module_draws() {
        for (name, source) in SOURCES {
            assert!(!source.contains("egui"), "flow/{name} reaches for egui");
        }
    }
}
