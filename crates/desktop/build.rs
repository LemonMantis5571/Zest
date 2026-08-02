use std::path::Path;
use std::process::Command;

fn main() {
    let dist = Path::new("ui/dist/index.html");
    if !dist.exists() {
        println!("cargo:warning=ui/dist missing — running npm run build --prefix ui");
        let status = Command::new("npm")
            .args(["run", "build", "--prefix", "ui"])
            .status()
            .expect("failed to spawn npm — install Node.js and run: npm install --prefix crates/desktop/ui");
        if !status.success() {
            panic!("npm run build --prefix ui failed; run it manually first");
        }
    }

    println!("cargo:rerun-if-changed=ui/dist/index.html");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
