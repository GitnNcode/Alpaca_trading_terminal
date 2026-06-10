// Build script — embeds the Windows .exe icon (assets/icon.ico) into the
// binary's resource section so Explorer / taskbar / Alt-Tab show the logo.
//
// It runs on every build but only does work when the *target* is Windows:
// build scripts execute on the host (macOS / Linux here), yet CARGO_CFG_TARGET_OS
// reflects the target, so the macOS universal build and the CI Linux build
// no-op straight through. No extra crate — we drive the MinGW `windres`
// directly, which keeps the build offline-safe and lets us sidestep the
// spaces-in-path bug (see the OUT_DIR note below).

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return; // only Windows targets carry an .exe icon
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Stage the icon + a one-line .rc inside OUT_DIR and run windres there.
    // OUT_DIR is space-free on our cross-build (the Windows build sets
    // CARGO_TARGET_DIR=/tmp/...), which matters because the MinGW windres
    // preprocessor mishandles spaces in paths — the same reason the Windows
    // build can't run from the repo's "Codeing stuff" path directly.
    let ico = out_dir.join("icon.ico");
    std::fs::copy(manifest.join("assets/icon.ico"), &ico)
        .expect("copy assets/icon.ico into OUT_DIR");
    // ID 1, type ICON. Windows uses the lowest-numbered group icon as the
    // application icon for the .exe.
    std::fs::write(out_dir.join("app.rc"), "1 ICON \"icon.ico\"\n").expect("write app.rc");

    let obj = out_dir.join("app_res.o");
    // For the GNU ABI use the MinGW cross prefix; bare `windres` covers an
    // eventual native/MSVC host that ships its own resource compiler.
    let windres = if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu") {
        "x86_64-w64-mingw32-windres"
    } else {
        "windres"
    };
    let status = Command::new(windres)
        .current_dir(&out_dir) // so the relative "icon.ico" resolves, no spaces
        .args(["-I", ".", "app.rc", "-O", "coff", "-o"])
        .arg(&obj)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {windres}: {e}"));
    assert!(status.success(), "{windres} failed to compile the icon resource");

    // Hand the compiled resource object straight to the linker for the binary.
    // A plain object (not an archive) is always linked in, so the .rsrc section
    // ships even though no symbol references it.
    println!("cargo:rustc-link-arg-bins={}", obj.display());
}
