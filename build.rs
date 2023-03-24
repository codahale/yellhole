fn main() {
    // trigger recompilation when a new migration is added
    println!("cargo:rerun-if-changed=migrations");

    // trigger recompilation when templates are changed
    println!("cargo:rerun-if-changed=templates");

    // trigger recompilation when assets are changed
    println!("cargo:rerun-if-changed=assets");

    vergen::EmitBuilder::builder().build_timestamp().emit().unwrap();
}
