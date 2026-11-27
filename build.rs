fn main() {
    // Embeds the app icon + DPI-aware manifest into the .exe (uses windres on the GNU toolchain).
    embed_resource::compile("assets/app.rc", embed_resource::NONE);
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/app.ico");
}
