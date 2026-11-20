fn main() {
    println!("cargo:rerun-if-changed=assets/DeskGrid.rc");
    println!("cargo:rerun-if-changed=assets/DeskGrid.ico");
    println!("cargo:rerun-if-changed=src/DeskGrid.manifest");
    println!("cargo:rerun-if-changed=LICENSE");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/DeskGrid.rc", embed_resource::NONE)
            .manifest_required()
            .expect("compile DeskGrid application resources");
    }
}
