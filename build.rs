use std::{env, path::Path, process::Command};

fn main() {
    for path in [
        "src",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        ".cargo/config.toml",
        "THIRD_PARTY_NOTICES.txt",
        "notices",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let timestamp = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse().ok())
        .and_then(|seconds| time::OffsetDateTime::from_unix_timestamp(seconds).ok())
        .unwrap_or_else(time::OffsetDateTime::now_utc);
    let timestamp = timestamp
        .replace_nanosecond(0)
        .expect("valid nanosecond")
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC build timestamp is representable");
    println!("cargo:rustc-env=SLX_BUILD_UTC={timestamp}");
    for (key, value) in [
        ("SLX_BUILD_TARGET", env::var("TARGET").unwrap()),
        ("SLX_BUILD_PROFILE", env::var("PROFILE").unwrap()),
    ] {
        println!("cargo:rustc-env={key}={value}");
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|| "Unavailable".into());
    println!("cargo:rustc-env=SLX_BUILD_REVISION={revision}");
    if Path::new(".git").exists() {
        println!("cargo:rerun-if-changed=.git/HEAD");
        if let Ok(reference) = std::fs::read_to_string(".git/HEAD")
            && let Some(reference) = reference.trim().strip_prefix("ref: ")
        {
            println!("cargo:rerun-if-changed=.git/{reference}");
        }
        println!("cargo:rerun-if-changed=.git/packed-refs");
    }
}
