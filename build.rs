use std::time::SystemTime;

fn main() {
    // trigger recompilation when a new migration is added
    println!("cargo:rerun-if-changed=migrations");

    // trigger recompilation when templates are changed
    println!("cargo:rerun-if-changed=templates");

    // trigger recompilation when assets are changed
    println!("cargo:rerun-if-changed=assets");

    // capture the build time as a Unix timestamp
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("should be a valid Unix timestamp");
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp.as_secs());
}
