use serde::de::DeserializeOwned;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

const RELEASE_PIN: &str = "gateway-release.json";
const LICENSE: &str = "licenses/CLIProxyAPI-LICENSE.txt";
const REQUIRED_TARGETS: [&str; 6] = [
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
];

#[derive(Debug, Deserialize)]
struct GatewayRelease {
    schema: u32,
    repository: String,
    version: String,
    targets: BTreeMap<String, GatewayTarget>,
}

#[derive(Debug, Deserialize)]
struct GatewayTarget {
    asset: String,
    sha256: String,
    binary_sha256: String,
}

#[derive(Debug, Deserialize)]
struct GatewayStamp {
    schema: u32,
    repository: String,
    version: String,
    target: String,
    asset: String,
    archive_sha256: String,
    binary_sha256: String,
}

fn main() {
    let release: GatewayRelease = read_json(Path::new(RELEASE_PIN));
    validate_release(&release);
    validate_license(Path::new(LICENSE));
    validate_sidecar_if_present(&release);

    let dist = Path::new("ui/dist/index.html");
    if !dist.exists() {
        println!("cargo:warning=ui/dist missing - running npm run build --prefix ui");
        let status = Command::new("npm")
            .args(["run", "build", "--prefix", "ui"])
            .status()
            .expect(
                "failed to spawn npm - install Node.js and run: npm install --prefix crates/desktop/ui",
            );
        if !status.success() {
            panic!("npm run build --prefix ui failed; run it manually first");
        }
    }

    println!("cargo:rerun-if-changed=ui/dist/index.html");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed={RELEASE_PIN}");
    println!("cargo:rerun-if-changed={LICENSE}");
    println!("cargo:rustc-env=ZEST_GATEWAY_VERSION={}", release.version);
    tauri_build::build();
}

fn read_json<T: DeserializeOwned>(path: &Path) -> T {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()))
}

fn validate_release(release: &GatewayRelease) {
    assert_eq!(release.schema, 1, "unsupported gateway release pin schema");
    assert_eq!(
        release.repository, "router-for-me/CLIProxyAPI",
        "gateway release pin names an unexpected repository"
    );
    assert!(
        !release.version.trim().is_empty(),
        "gateway release pin has no version"
    );
    assert_eq!(
        release.targets.len(),
        REQUIRED_TARGETS.len(),
        "gateway release pin has an unexpected target set"
    );

    let expected_prefix = format!("CLIProxyAPI_{}_", release.version);
    for target in REQUIRED_TARGETS {
        let entry = release
            .targets
            .get(target)
            .unwrap_or_else(|| panic!("gateway release pin has no entry for {target}"));
        assert_eq!(
            Path::new(&entry.asset)
                .file_name()
                .and_then(|name| name.to_str()),
            Some(entry.asset.as_str()),
            "gateway asset for {target} must be a file name, not a path"
        );
        assert!(
            entry.asset.starts_with(&expected_prefix),
            "gateway asset for {target} does not match pinned version {}",
            release.version
        );
        let expected_arch = if target.starts_with("x86_64-") {
            "_amd64"
        } else {
            "_aarch64"
        };
        assert!(
            entry.asset.contains(expected_arch),
            "gateway asset for {target} does not match its architecture"
        );
        assert_sha256(&entry.sha256, &format!("archive hash for {target}"));
        assert_sha256(&entry.binary_sha256, &format!("binary hash for {target}"));
    }
}

fn validate_license(path: &Path) {
    let text = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("CLIProxyAPI licence is missing or unreadable: {e}"));
    assert!(
        text.contains("MIT License")
            && text.contains("Copyright (c) 2025.9-present Router-For.ME")
            && text.contains("Permission is hereby granted"),
        "CLIProxyAPI licence is incomplete or unexpected"
    );
}

fn validate_sidecar_if_present(release: &GatewayRelease) {
    let target = env::var("TARGET").expect("Cargo did not provide TARGET");
    let pinned = release
        .targets
        .get(&target)
        .unwrap_or_else(|| panic!("no pinned CLIProxyAPI asset for build target {target}"));
    let extension = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let sidecar = PathBuf::from(format!("binaries/cli-proxy-api-{target}{extension}"));
    let stamp_path = PathBuf::from(format!("{}.source.json", sidecar.display()));

    println!("cargo:rerun-if-changed={}", sidecar.display());
    println!("cargo:rerun-if-changed={}", stamp_path.display());

    if !sidecar.exists() {
        assert!(
            env::var("PROFILE").as_deref() != Ok("release"),
            "release builds require the pinned CLIProxyAPI sidecar; run scripts/fetch-gateway.ps1"
        );
        assert!(
            !stamp_path.exists(),
            "gateway provenance exists without its sidecar: {}",
            stamp_path.display()
        );
        return;
    }

    assert!(
        sidecar.is_file(),
        "gateway sidecar path is not a file: {}",
        sidecar.display()
    );
    assert!(
        stamp_path.is_file(),
        "gateway sidecar has no verified provenance; run scripts/fetch-gateway.ps1"
    );
    let stamp: GatewayStamp = read_json(&stamp_path);
    assert_eq!(stamp.schema, 1, "unsupported gateway provenance schema");
    assert_eq!(
        stamp.repository, release.repository,
        "gateway repository drift"
    );
    assert_eq!(stamp.version, release.version, "gateway version drift");
    assert_eq!(stamp.target, target, "gateway target drift");
    assert_eq!(stamp.asset, pinned.asset, "gateway release asset drift");
    assert_eq!(
        stamp.archive_sha256.to_ascii_lowercase(),
        pinned.sha256.to_ascii_lowercase(),
        "gateway archive hash drift"
    );
    assert_sha256(&stamp.binary_sha256, "gateway binary hash");
    assert_eq!(
        sha256_file(&sidecar),
        pinned.binary_sha256.to_ascii_lowercase(),
        "gateway sidecar is corrupt or was replaced; run scripts/fetch-gateway.ps1 -Force"
    );
    assert_eq!(
        stamp.binary_sha256.to_ascii_lowercase(),
        pinned.binary_sha256.to_ascii_lowercase(),
        "gateway provenance hash drift"
    );
}

fn assert_sha256(value: &str, label: &str) {
    assert!(
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{label} is not a SHA256 digest"
    );
}

fn sha256_file(path: &Path) -> String {
    let mut file = File::open(path)
        .unwrap_or_else(|e| panic!("could not open gateway sidecar {}: {e}", path.display()));
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .unwrap_or_else(|e| panic!("could not hash gateway sidecar {}: {e}", path.display()));
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    format!("{:x}", digest.finalize())
}
