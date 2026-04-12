# etr

eBPF-based transit router service with a Rust control plane and a TC-first forwarding path.

## MVP Direction

- Data plane: hybrid architecture, TC backend first, XDP later
- Protocols: TCP and UDP
- Address family: IPv4 first, IPv6-ready config model
- Return path: SNAT/MASQUERADE by default
- Control plane: static config file as source of truth plus HTTP reload API
- Compatibility target: Linux 5.15+ with usable `vmlinux BTF`

## Repository Layout

```text
config/               Example runtime configuration and local overrides
crates/etr-types/     Shared kernel/user-space wire types for TC maps and flow state
crates/etr-config/    Shared config schema and validation
crates/etr-control/   Control-plane runtime and data-plane abstraction
crates/etrd/          Daemon entrypoint and management HTTP API
ebpf/etr-ebpf/        Aya-based TC ingress/egress programs
```

## Example Config

See [`config/etr.toml.example`](./config/etr.toml.example) for a point-to-point IPv4 forwarding example.

Each rule currently:

- accepts exactly one backend
- supports `tcp` or `udp`
- requires `family = "ipv4"`
- uses `snat = "masquerade"`
- requires `[data_plane].external_interface`

The schema intentionally keeps `backends = [...]` as a list-shaped structure so the model can grow into multi-backend load balancing later.

## Management API

The daemon exposes:

- `GET /healthz` for basic health and current rule count
- `GET /api/v1/config` for the in-memory snapshot
- `GET /api/v1/status` for runtime status including data-plane debug state
- `GET /api/v1/debug/dataplane` for preflight results, flow counts, and runtime counters
- `POST /api/v1/admin/reload` to reload the config file and re-apply rules

## Running The Control Plane

Create a local config from the example before running the daemon:

```bash
cp config/etr.toml.example config/etr.toml
cargo run -p etrd -- --config config/etr.toml
```

Then reload config changes:

```bash
curl -X POST http://127.0.0.1:9911/api/v1/admin/reload
```

On non-Linux development machines, the daemon falls back to a safe `tc-stub` backend so the config and reload path can still be exercised.
On Linux, the daemon also falls back to `tc-stub` if you do not pass `--bpf-object`. That is useful for validating config parsing and the management API before the real eBPF object is built.

## Linux TC Path

On Linux, `etrd` can consume a compiled eBPF object and use the Aya-backed TC data plane instead of the stub backend.

For real-host TCP/UDP validation steps, see [`docs/linux-validation.md`](./docs/linux-validation.md).

Current startup shape:

```bash
etrd --config config/etr.toml --bpf-object /path/to/etr-ebpf-object
```

Linux TC startup now runs productized preflight checks before dataplane activation.
Critical failures stop startup by default. If you intentionally want degraded bring-up for debugging, use:

```bash
etrd --config config/etr.toml \
  --bpf-object /path/to/etr-ebpf-object \
  --allow-preflight-warnings
```

Current implementation assumptions:

- attaches `etr_ingress` to TC ingress on `[data_plane].external_interface`
- attaches `etr_egress` to TC egress on the same interface
- syncs the `ETR_TC_FORWARD_RULES` and `ETR_TC_FLOW_STATE` maps from user-space config
- exports runtime counters through the `ETR_TC_RUNTIME_STATS` map
- performs forward `DNAT` on ingress, forward `SNAT/MASQUERADE` on egress, and reverse NAT on ingress for backend replies

Frontend ports are not local user-space listeners.
`etr` rewrites traffic that enters and leaves `[data_plane].external_interface`, so `curl 127.0.0.1:<frontend_port>` is expected to fail with `Connection refused`.
Validate forwarding with traffic sent to the host's real interface address or public IP from another machine, not through loopback.

Linux forwarding prerequisites:

- `net.ipv4.ip_forward = 1`
- `net.ipv4.conf.<external_interface>.rp_filter = 0` or `2`
- `/sys/kernel/btf/vmlinux` must be present
- the process must run with privileges sufficient for TC attach and BPF map access
- `[data_plane].external_interface` must exist and be operational
- upstream cloud security groups or host firewalls must allow the exposed frontend ports

Persist the host routing baseline through a dedicated sysctl file instead of `etr` config.
One workable pattern is:

```bash
cat <<'EOF' | sudo tee /etc/sysctl.d/99-ip-forward.conf
net.ipv4.ip_forward = 1
net.ipv4.conf.eth0.rp_filter = 0
EOF

sudo sysctl --system
```

Replace `eth0` with the same interface configured in `[data_plane].external_interface`.
Avoid defining the same sysctl key in multiple files such as `/etc/sysctl.conf` and `/etc/sysctl.d/*.conf`, otherwise the lexicographically later entry wins and may override the intended value.

If preflight is overridden, `/healthz` reports `degraded` and `/api/v1/debug/dataplane` exposes the failing checks.

## Versioning And Release Workflow

The workspace version in [`Cargo.toml`](./Cargo.toml) is the only version source.
The format is `YYMM.D.BUILD`, for example `2604.13.1`.

Bump the workspace version:

```bash
./scripts/bump_version.py --manifest-path Cargo.toml
```

Behavior:

- if the existing version already matches today's `YYMM.D`, increment `BUILD`
- otherwise rewrite the version to today's `YYMM.D.1`

Build a versioned release bundle:

```bash
./scripts/build_release.sh
```

This produces an artifact like:

```text
target/release-bundle/etr-v2604.13.1-linux-x86_64.tar.gz
```

Bundle layout:

```text
bin/etrd
lib/etr/etr-ebpf
config/etr.toml.example
share/etr/install.sh
```

## Linux Build Notes

The eBPF object is built from the `etr-ebpf` binary target, not from a Rust library artifact.

One-time setup on Linux:

```bash
rustup toolchain install nightly --component rust-src
cargo install bpf-linker
```

Build the eBPF object:

```bash
RUSTFLAGS="-C debuginfo=2 -C link-arg=--btf" \
CARGO_TARGET_BPFEL_UNKNOWN_NONE_LINKER=bpf-linker \
cargo +nightly build -p etr-ebpf \
  --target bpfel-unknown-none \
  -Z build-std=core \
  --release
```

Expected output path:

```bash
target/bpfel-unknown-none/release/etr-ebpf
```

Run the daemon against that object:

```bash
cargo run -p etrd -- \
  --config config/etr.toml \
  --bpf-object target/bpfel-unknown-none/release/etr-ebpf
```

If you omit `--bpf-object`, Linux falls back to the safe `tc-stub` backend so you can still validate config parsing and the management API.

## Installation

Unpack the release bundle and install using either entry point:

```bash
sudo ./share/etr/install.sh
```

or:

```bash
sudo ./bin/etrd install
```

To start the service immediately:

```bash
sudo ./bin/etrd install --start
```

The canonical installation logic lives in `etrd install`, which:

- installs `etrd` to `/usr/local/bin/etrd`
- installs the eBPF object to `/usr/local/lib/etr/etr-ebpf`
- installs the config example to `/etc/etr/etr.toml` if that file does not already exist
- creates `/var/lib/etr`
- installs a service definition using `systemd` when available, otherwise `/etc/init.d/etrd`

After installation, `etrd install` prints the detected service manager and the exact command to check service status.

Remove the installed service and binaries:

```bash
sudo /usr/local/bin/etrd uninstall
```

Also remove config and runtime state:

```bash
sudo /usr/local/bin/etrd uninstall --purge
```

## Deployment Notes

The current repository now contains:

- a Rust control plane that loads config, exposes reload APIs, and selects a data-plane backend
- shared map key/value types used by both user-space and eBPF code
- a Linux-only Aya TC backend loader in `etr-control`
- a first-pass TC ingress/egress eBPF implementation in `ebpf/etr-ebpf`
- a release-bundle workflow with versioned artifacts
- canonical install and uninstall flows for `systemd` and init.d-style hosts
- startup preflight diagnostics plus JSON dataplane debug/status surfaces
