#![cfg(target_os = "linux")]

use crate::dataplane::{PreflightCheck, PreflightReport};
use std::fs;
use std::path::Path;

pub(crate) fn run_linux_preflight(
    interface: &str,
    object_path: &Path,
    override_active: bool,
) -> PreflightReport {
    let checks = vec![
        check_root(),
        check_object_path(object_path),
        check_btf(),
        check_interface_exists(interface),
        check_interface_state(interface),
        check_ip_forward(),
        check_rp_filter(interface),
    ];

    let passed = checks.iter().all(|check| check.ok || !check.critical);

    PreflightReport {
        passed,
        override_active,
        checks,
    }
}

fn check_root() -> PreflightCheck {
    let euid = current_euid();
    if euid == 0 {
        ok_check("root_privileges", "running with root privileges".to_owned())
    } else {
        fail_check(
            "root_privileges",
            format!("running as uid {euid}; TC attach and map operations require root"),
            Some("run etrd as root or through a system service with sufficient privileges"),
        )
    }
}

fn check_object_path(object_path: &Path) -> PreflightCheck {
    if object_path.is_file() {
        ok_check(
            "bpf_object",
            format!("found eBPF object at {}", object_path.display()),
        )
    } else {
        fail_check(
            "bpf_object",
            format!("missing eBPF object at {}", object_path.display()),
            Some("build or install the etr-ebpf artifact and pass the correct --bpf-object path"),
        )
    }
}

fn check_btf() -> PreflightCheck {
    let path = Path::new("/sys/kernel/btf/vmlinux");
    if path.is_file() {
        ok_check(
            "kernel_btf",
            format!("found kernel BTF at {}", path.display()),
        )
    } else {
        fail_check(
            "kernel_btf",
            format!("missing kernel BTF at {}", path.display()),
            Some("use a supported Linux kernel that exposes /sys/kernel/btf/vmlinux"),
        )
    }
}

fn check_interface_exists(interface: &str) -> PreflightCheck {
    let path = Path::new("/sys/class/net").join(interface);
    if path.exists() {
        ok_check(
            "external_interface_exists",
            format!("found interface {}", interface),
        )
    } else {
        fail_check(
            "external_interface_exists",
            format!("interface {} does not exist", interface),
            Some("set data_plane.external_interface to an existing Linux network interface"),
        )
    }
}

fn check_interface_state(interface: &str) -> PreflightCheck {
    let path = Path::new("/sys/class/net")
        .join(interface)
        .join("operstate");
    match read_trimmed(&path) {
        Ok(value) if value == "up" || value == "unknown" => ok_check(
            "external_interface_state",
            format!("interface {} operstate is {}", interface, value),
        ),
        Ok(value) => fail_check(
            "external_interface_state",
            format!("interface {} operstate is {}", interface, value),
            Some("bring the external interface up before starting etrd"),
        ),
        Err(error) => fail_check(
            "external_interface_state",
            format!("failed to read {}: {error}", path.display()),
            Some("verify that the external interface exists and sysfs is accessible"),
        ),
    }
}

fn check_ip_forward() -> PreflightCheck {
    let path = Path::new("/proc/sys/net/ipv4/ip_forward");
    match read_trimmed(path) {
        Ok(value) if value == "1" => {
            ok_check("ip_forward", "net.ipv4.ip_forward is enabled".to_owned())
        }
        Ok(value) => fail_check(
            "ip_forward",
            format!("net.ipv4.ip_forward is {value}"),
            Some("set net.ipv4.ip_forward = 1"),
        ),
        Err(error) => fail_check(
            "ip_forward",
            format!("failed to read {}: {error}", path.display()),
            Some("verify /proc/sys is mounted and readable"),
        ),
    }
}

fn check_rp_filter(interface: &str) -> PreflightCheck {
    let path = Path::new("/proc/sys/net/ipv4/conf")
        .join(interface)
        .join("rp_filter");
    match read_trimmed(&path) {
        Ok(value) if value == "0" || value == "2" => ok_check(
            "rp_filter",
            format!("net.ipv4.conf.{interface}.rp_filter is {value}"),
        ),
        Ok(value) => fail_check(
            "rp_filter",
            format!("net.ipv4.conf.{interface}.rp_filter is {value}"),
            Some("set rp_filter to 0 or 2 on the external interface"),
        ),
        Err(error) => fail_check(
            "rp_filter",
            format!("failed to read {}: {error}", path.display()),
            Some("verify the external interface exists and procfs is accessible"),
        ),
    }
}

fn read_trimmed(path: &Path) -> Result<String, std::io::Error> {
    fs::read_to_string(path).map(|value| value.trim().to_owned())
}

fn ok_check(name: &str, message: String) -> PreflightCheck {
    PreflightCheck {
        name: name.to_owned(),
        ok: true,
        critical: true,
        message,
        remediation: None,
    }
}

fn fail_check(name: &str, message: String, remediation: Option<&str>) -> PreflightCheck {
    PreflightCheck {
        name: name.to_owned(),
        ok: false,
        critical: true,
        message,
        remediation: remediation.map(str::to_owned),
    }
}

fn current_euid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    unsafe { geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_fails_when_any_critical_check_fails() {
        let report = PreflightReport {
            passed: false,
            override_active: true,
            checks: vec![fail_check(
                "ip_forward",
                "disabled".to_owned(),
                Some("enable it"),
            )],
        };

        assert_eq!(report.summary(), "ip_forward: disabled");
    }

    #[test]
    fn ok_check_has_no_remediation() {
        let check = ok_check("kernel_btf", "present".to_owned());

        assert!(check.ok);
        assert_eq!(check.remediation, None);
    }
}
