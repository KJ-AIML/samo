//! On Windows, Samo's icon goes into the program as icon 1, which Explorer shows and GPUI loads for the window
//! and the taskbar (ADR-006). Elsewhere this does nothing.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=samo.rc");
        println!("cargo:rerun-if-changed=assets/Samo.ico");
        if let Err(e) = embed_resource::compile("samo.rc", embed_resource::NONE).manifest_optional()
        {
            println!("cargo:warning=Samo's icon wasn't embedded: {e}");
        }
    }
}
