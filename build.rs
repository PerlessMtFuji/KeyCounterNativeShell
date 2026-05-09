// build.rs — Windows-only build: embeds the application manifest (DPI
// awareness, common controls v6, long path support) and the icon
// resources, and sets the `windows` subsystem so release builds don't
// flash a console window on launch.
//
// We deliberately don't gate on `target_os = "windows"` — this crate
// is Windows-only by design, so failing fast on other targets is
// preferable to pretending to support them.

use std::env;

fn main() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        // Refuse to build on non-Windows. The whole UI layer is
        // Win32 + Direct2D — nothing meaningful would run.
        panic!(
            "keycounter-native targets Windows only; current target_os = {}",
            target_os
        );
    }

    // Re-run only when the manifest or any embedded resource changes —
    // pinning these saves a ~2 s rebuild step on every `cargo check`.
    println!("cargo:rerun-if-changed=resources/app.manifest");
    println!("cargo:rerun-if-changed=resources/icon.ico");
    println!("cargo:rerun-if-changed=resources/tray-idle.ico");
    println!("cargo:rerun-if-changed=resources/tray-pulse.ico");
    println!("cargo:rerun-if-changed=resources/keycounter.rc");

    // Skip resource embedding entirely for `cargo check` — winres
    // shells out to rc.exe / windres which dominates the type-check
    // cycle. Only embed for actual builds.
    if env::var("CARGO_CFG_RUSTDOC").is_ok() {
        return;
    }

    // The .rc file pulls in the manifest + the icon group. embed-resource
    // handles the toolchain selection (rc.exe on MSVC, windres on GNU)
    // transparently.
    let res_path = std::path::Path::new("resources/keycounter.rc");
    if res_path.exists() {
        embed_resource::compile(res_path, embed_resource::NONE);
    }

    // Subsystem: hide the console on release. winres sets the linker
    // flag in a way that survives both MSVC and GNU toolchains.
    let profile = env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg-bins=/ENTRY:mainCRTStartup");
    }
}
