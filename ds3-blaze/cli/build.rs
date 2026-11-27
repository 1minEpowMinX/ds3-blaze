//! Embeds [`ICON`] in the Windows executable.

/// Icon of the executable.
const ICON: &str = "../assets/ds3-blaze-cli.ico";

fn main() {
    println!("cargo:rerun-if-changed={ICON}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon(ICON)
            // Task Manager shows the description instead of the file name, so
            // it stays a short name.
            .set("FileDescription", "ds3-blaze command line")
            .set("ProductName", "ds3-blaze")
            .compile()
            .expect("embedding the icon failed");
    }
}
