// the report says which anybox it ran on, and Cargo.lock is the only place that knows the exact version
fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    let lock = std::fs::read_to_string("Cargo.lock").unwrap_or_default();
    let version = lock
        .split("[[package]]")
        .find(|package| package.contains("name = \"anybox-rs\""))
        .and_then(|package| package.lines().find_map(|l| l.strip_prefix("version = ")))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=ANYBOX_VERSION={version}");
}
