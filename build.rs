// Bake version metadata into the binary so /health can report what's
// actually running.
//
// Three values are exposed as compile-time `env!()` constants:
//   APP_VERSION  — the human-readable version (e.g. "0.5.2")
//   GIT_SHA      — short commit SHA the binary was built from
//   BUILT_AT     — UTC timestamp the build started (RFC 3339)
//
// CI populates all three via `--build-arg` on `docker build`. Local builds
// fall back gracefully so `cargo build` works without any setup:
//   APP_VERSION → CARGO_PKG_VERSION (Cargo.toml)
//   GIT_SHA     → live `git rev-parse --short HEAD`
//   BUILT_AT    → "unknown" (chrono not in build-dependencies)

fn main() {
    println!("cargo:rerun-if-env-changed=APP_VERSION");
    println!("cargo:rerun-if-env-changed=GIT_SHA");
    println!("cargo:rerun-if-env-changed=BUILT_AT");

    let cargo_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".into());
    let version = std::env::var("APP_VERSION").unwrap_or(cargo_version);

    let commit = std::env::var("GIT_SHA").ok().unwrap_or_else(|| {
        std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    });

    let built_at = std::env::var("BUILT_AT").unwrap_or_else(|_| "unknown".into());

    println!("cargo:rustc-env=APP_VERSION={}", version);
    println!("cargo:rustc-env=GIT_SHA={}", commit);
    println!("cargo:rustc-env=BUILT_AT={}", built_at);
}
