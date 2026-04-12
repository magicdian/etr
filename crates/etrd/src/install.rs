use anyhow::{Context, bail};
use clap::Args;
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const INSTALL_BIN_PATH: &str = "/usr/local/bin/etrd";
const INSTALL_BPF_PATH: &str = "/usr/local/lib/etr/etr-ebpf";
const INSTALL_CONFIG_PATH: &str = "/etc/etr/etr.toml";
const INSTALL_STATE_DIR: &str = "/var/lib/etr";
const SYSTEMD_UNIT_PATH: &str = "/etc/systemd/system/etrd.service";
const INITD_SCRIPT_PATH: &str = "/etc/init.d/etrd";
const PIDFILE_PATH: &str = "/run/etrd.pid";

#[derive(Debug, Args)]
pub(crate) struct InstallArgs {
    #[arg(long, help = "Path to the eBPF object to install")]
    bpf_object_source: Option<PathBuf>,
    #[arg(long, help = "Path to the config example to install")]
    config_source: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = false,
        help = "Overwrite /etc/etr/etr.toml if it already exists"
    )]
    force: bool,
    #[arg(
        long,
        default_value_t = false,
        help = "Start the service after installation"
    )]
    start: bool,
}

#[derive(Debug, Args)]
pub(crate) struct UninstallArgs {
    #[arg(
        long,
        default_value_t = false,
        help = "Also remove config and runtime state"
    )]
    purge: bool,
}

pub(crate) fn install(args: InstallArgs) -> anyhow::Result<()> {
    ensure_root()?;

    let current_exe = env::current_exe().context("failed to resolve current executable path")?;
    let bundle_root = detect_bundle_root(&current_exe);
    let bpf_source = resolve_bpf_source(args.bpf_object_source, bundle_root.as_deref())?;
    let config_source = resolve_config_source(args.config_source, bundle_root.as_deref())?;

    create_parent_dir(Path::new(INSTALL_BIN_PATH))?;
    create_parent_dir(Path::new(INSTALL_BPF_PATH))?;
    create_parent_dir(Path::new(INSTALL_CONFIG_PATH))?;
    fs::create_dir_all(INSTALL_STATE_DIR)
        .with_context(|| format!("failed to create {}", INSTALL_STATE_DIR))?;

    install_file(&current_exe, Path::new(INSTALL_BIN_PATH), 0o755)?;
    install_file(&bpf_source, Path::new(INSTALL_BPF_PATH), 0o644)?;
    install_config(&config_source, args.force)?;

    let service_manager = detect_service_manager()?;
    let install_outcome = match service_manager {
        ServiceManager::Systemd => install_systemd_service(args.start)?,
        ServiceManager::InitD => install_initd_service(args.start)?,
    };

    println!("Installed etrd to {}", INSTALL_BIN_PATH);
    println!("Installed eBPF object to {}", INSTALL_BPF_PATH);
    println!("Config path: {}", INSTALL_CONFIG_PATH);
    println!("{}", install_outcome.summary());

    Ok(())
}

pub(crate) fn uninstall(args: UninstallArgs) -> anyhow::Result<()> {
    ensure_root()?;

    if Path::new(SYSTEMD_UNIT_PATH).exists() {
        uninstall_systemd_service()?;
    } else if Path::new(INITD_SCRIPT_PATH).exists() {
        uninstall_initd_service()?;
    }

    remove_if_exists(Path::new(INSTALL_BIN_PATH))?;
    remove_if_exists(Path::new(INSTALL_BPF_PATH))?;

    if args.purge {
        remove_if_exists(Path::new(INSTALL_CONFIG_PATH))?;
        remove_dir_if_exists(Path::new(INSTALL_STATE_DIR))?;
    }

    println!("Removed installed etrd artifacts");
    Ok(())
}

fn resolve_bpf_source(
    explicit: Option<PathBuf>,
    bundle_root: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        ensure_file_exists(&path, "bpf object")?;
        return Ok(path);
    }

    if let Some(root) = bundle_root {
        let candidate = root.join("lib/etr/etr-ebpf");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    bail!(
        "unable to locate eBPF object; pass --bpf-object-source or run install from a release bundle"
    )
}

fn resolve_config_source(
    explicit: Option<PathBuf>,
    bundle_root: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        ensure_file_exists(&path, "config example")?;
        return Ok(path);
    }

    if let Some(root) = bundle_root {
        let candidate = root.join("config/etr.toml.example");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let repo_candidate = PathBuf::from("config/etr.toml.example");
    if repo_candidate.is_file() {
        return Ok(repo_candidate);
    }

    bail!(
        "unable to locate config example; pass --config-source or run install from a release bundle"
    )
}

fn detect_bundle_root(current_exe: &Path) -> Option<PathBuf> {
    let bin_dir = current_exe.parent()?;
    if bin_dir.file_name()? != "bin" {
        return None;
    }
    Some(bin_dir.parent()?.to_path_buf())
}

fn install_config(source: &Path, force: bool) -> anyhow::Result<()> {
    if Path::new(INSTALL_CONFIG_PATH).exists() && !force {
        println!(
            "Keeping existing config at {} (use --force to overwrite)",
            INSTALL_CONFIG_PATH
        );
        return Ok(());
    }

    install_file(source, Path::new(INSTALL_CONFIG_PATH), 0o644)
}

fn install_systemd_service(start: bool) -> anyhow::Result<InstallOutcome> {
    write_text_file(Path::new(SYSTEMD_UNIT_PATH), &render_systemd_unit(), 0o644)?;
    run_command(
        Command::new("systemctl").arg("daemon-reload"),
        "systemctl daemon-reload",
    )?;
    run_command(
        Command::new("systemctl").arg("enable").arg("etrd.service"),
        "systemctl enable etrd.service",
    )?;
    if start {
        run_command(
            Command::new("systemctl").arg("restart").arg("etrd.service"),
            "systemctl restart etrd.service",
        )?;
    }
    Ok(InstallOutcome {
        manager_label: "Detected service manager: systemd",
        unit_path: SYSTEMD_UNIT_PATH,
        status_hint: if start {
            "Service etrd.service was enabled and restarted.\nCheck status with: systemctl status etrd.service"
        } else {
            "Service etrd.service was enabled but not started.\nStart it with: systemctl start etrd.service\nCheck status with: systemctl status etrd.service"
        },
    })
}

fn uninstall_systemd_service() -> anyhow::Result<()> {
    let _ = run_command(
        Command::new("systemctl")
            .arg("disable")
            .arg("--now")
            .arg("etrd.service"),
        "systemctl disable --now etrd.service",
    );
    remove_if_exists(Path::new(SYSTEMD_UNIT_PATH))?;
    let _ = run_command(
        Command::new("systemctl").arg("daemon-reload"),
        "systemctl daemon-reload",
    );
    Ok(())
}

fn install_initd_service(start: bool) -> anyhow::Result<InstallOutcome> {
    write_text_file(Path::new(INITD_SCRIPT_PATH), &render_initd_script(), 0o755)?;
    if command_exists("update-rc.d") {
        let _ = run_command(
            Command::new("update-rc.d").arg("etrd").arg("defaults"),
            "update-rc.d etrd defaults",
        );
    } else if command_exists("chkconfig") {
        let _ = run_command(
            Command::new("chkconfig").arg("--add").arg("etrd"),
            "chkconfig --add etrd",
        );
    }
    if start {
        run_command(
            Command::new(INITD_SCRIPT_PATH).arg("restart"),
            "init.d restart etrd",
        )?;
    }
    Ok(InstallOutcome {
        manager_label: "Detected service manager: init.d",
        unit_path: INITD_SCRIPT_PATH,
        status_hint: if start {
            "Service etrd was installed and restarted.\nCheck status with: /etc/init.d/etrd status"
        } else {
            "Service etrd was installed but not started.\nStart it with: /etc/init.d/etrd start\nCheck status with: /etc/init.d/etrd status"
        },
    })
}

fn uninstall_initd_service() -> anyhow::Result<()> {
    let _ = run_command(
        Command::new(INITD_SCRIPT_PATH).arg("stop"),
        "init.d stop etrd",
    );
    if command_exists("update-rc.d") {
        let _ = run_command(
            Command::new("update-rc.d")
                .arg("-f")
                .arg("etrd")
                .arg("remove"),
            "update-rc.d -f etrd remove",
        );
    } else if command_exists("chkconfig") {
        let _ = run_command(
            Command::new("chkconfig").arg("--del").arg("etrd"),
            "chkconfig --del etrd",
        );
    }
    remove_if_exists(Path::new(INITD_SCRIPT_PATH))?;
    remove_if_exists(Path::new(PIDFILE_PATH))?;
    Ok(())
}

fn detect_service_manager() -> anyhow::Result<ServiceManager> {
    if Path::new("/run/systemd/system").exists() && command_exists("systemctl") {
        return Ok(ServiceManager::Systemd);
    }

    if Path::new("/etc/init.d").exists() {
        return Ok(ServiceManager::InitD);
    }

    bail!("no supported service manager detected; expected systemd or /etc/init.d")
}

fn install_file(source: &Path, target: &Path, mode: u32) -> anyhow::Result<()> {
    ensure_file_exists(source, "source file")?;
    if source == target {
        set_mode(target, mode)?;
        return Ok(());
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            target.display()
        )
    })?;
    set_mode(target, mode)
}

fn write_text_file(path: &Path, contents: &str, mode: u32) -> anyhow::Result<()> {
    create_parent_dir(path)?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    set_mode(path, mode)
}

fn create_parent_dir(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path {} has no parent", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))
}

fn set_mode(path: &Path, mode: u32) -> anyhow::Result<()> {
    let mut perms = fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?
        .permissions();
    perms.set_mode(mode);
    fs::set_permissions(path, perms)
        .with_context(|| format!("failed to set permissions on {}", path.display()))
}

fn ensure_file_exists(path: &Path, label: &str) -> anyhow::Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("{label} not found at {}", path.display())
    }
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn remove_dir_if_exists(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove directory {}", path.display()))?;
    }
    Ok(())
}

fn run_command(command: &mut Command, label: &str) -> anyhow::Result<()> {
    let output = command
        .output()
        .with_context(|| format!("failed to execute {label}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("{label} failed: {}", stderr.trim())
}

fn command_exists(name: &str) -> bool {
    env::var_os("PATH")
        .is_some_and(|paths| env::split_paths(&paths).any(|dir| dir.join(name).is_file()))
}

fn ensure_root() -> anyhow::Result<()> {
    if current_euid() == 0 {
        return Ok(());
    }
    bail!("install and uninstall require root privileges")
}

fn current_euid() -> u32 {
    unsafe extern "C" {
        fn geteuid() -> u32;
    }

    unsafe { geteuid() }
}

fn render_systemd_unit() -> String {
    format!(
        "[Unit]\nDescription=etr control-plane daemon\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nExecStart={} --config {} --bpf-object {}\nWorkingDirectory={}\nRestart=on-failure\nRestartSec=2\n\n[Install]\nWantedBy=multi-user.target\n",
        INSTALL_BIN_PATH, INSTALL_CONFIG_PATH, INSTALL_BPF_PATH, INSTALL_STATE_DIR
    )
}

fn render_initd_script() -> String {
    format!(
        "#!/bin/sh\n### BEGIN INIT INFO\n# Provides:          etrd\n# Required-Start:    $network\n# Required-Stop:     $network\n# Default-Start:     2 3 4 5\n# Default-Stop:      0 1 6\n# Short-Description: etr control-plane daemon\n### END INIT INFO\n\nDAEMON=\"{}\"\nARGS=\"--config {} --bpf-object {}\"\nPIDFILE=\"{}\"\n\nstart() {{\n  if command -v start-stop-daemon >/dev/null 2>&1; then\n    start-stop-daemon --start --quiet --background --make-pidfile --pidfile \"$PIDFILE\" --exec \"$DAEMON\" -- $ARGS\n  else\n    nohup \"$DAEMON\" $ARGS >/var/log/etrd.log 2>&1 &\n    echo $! > \"$PIDFILE\"\n  fi\n}}\n\nstop() {{\n  if command -v start-stop-daemon >/dev/null 2>&1; then\n    start-stop-daemon --stop --quiet --pidfile \"$PIDFILE\" --retry 5 || true\n  elif [ -f \"$PIDFILE\" ]; then\n    kill \"$(cat \"$PIDFILE\")\" || true\n  fi\n  rm -f \"$PIDFILE\"\n}}\n\ncase \"$1\" in\n  start)\n    start\n    ;;\n  stop)\n    stop\n    ;;\n  restart)\n    stop\n    start\n    ;;\n  status)\n    if [ -f \"$PIDFILE\" ]; then\n      echo \"etrd running with pid $(cat \"$PIDFILE\")\"\n    else\n      echo \"etrd not running\"\n      exit 3\n    fi\n    ;;\n  *)\n    echo \"Usage: $0 {{start|stop|restart|status}}\"\n    exit 2\n    ;;\nesac\n",
        INSTALL_BIN_PATH, INSTALL_CONFIG_PATH, INSTALL_BPF_PATH, PIDFILE_PATH
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServiceManager {
    Systemd,
    InitD,
}

struct InstallOutcome {
    manager_label: &'static str,
    unit_path: &'static str,
    status_hint: &'static str,
}

impl InstallOutcome {
    fn summary(&self) -> String {
        format!(
            "{}\nService definition path: {}\n{}",
            self.manager_label, self.unit_path, self.status_hint
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_root_detects_bin_layout() {
        let path = Path::new("/tmp/etr-v2604.13.1-linux-x86_64/bin/etrd");
        assert_eq!(
            detect_bundle_root(path),
            Some(PathBuf::from("/tmp/etr-v2604.13.1-linux-x86_64"))
        );
    }

    #[test]
    fn systemd_unit_uses_installed_paths() {
        let unit = render_systemd_unit();

        assert!(unit.contains(INSTALL_BIN_PATH));
        assert!(unit.contains(INSTALL_CONFIG_PATH));
        assert!(unit.contains(INSTALL_BPF_PATH));
    }

    #[test]
    fn initd_script_uses_installed_paths() {
        let script = render_initd_script();

        assert!(script.contains(INSTALL_BIN_PATH));
        assert!(script.contains(INSTALL_CONFIG_PATH));
        assert!(script.contains(INSTALL_BPF_PATH));
        assert!(script.contains(PIDFILE_PATH));
    }
}
