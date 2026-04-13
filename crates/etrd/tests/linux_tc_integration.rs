#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::process::Command;

#[test]
#[ignore = "requires root, network namespaces, and Linux eBPF toolchain"]
fn forwards_tcp_and_udp_through_linux_tc_dataplane() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("etrd crate should live under crates/etrd");
    let script_path = repo_root.join("scripts/run_linux_tc_integration.sh");

    let output = Command::new("bash")
        .arg(script_path)
        .env("ETR_TEST_REPO_ROOT", repo_root)
        .env("ETR_TEST_ETRD_BIN", env!("CARGO_BIN_EXE_etrd"))
        .output()
        .expect("failed to execute linux tc integration harness");

    if !output.status.success() {
        panic!(
            "linux tc integration harness failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
