use std::process::Command;

fn main() {
    // Re-run when HEAD moves so `ddb --version` reflects the actual
    // built commit instead of cargo's cached env from the previous
    // build script run.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=DDB_GIT_HASH={}", hash.trim());
}
