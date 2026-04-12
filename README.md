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
config/               Example runtime configuration
crates/etr-types/     Shared kernel/user-space wire types for TC maps and flow state
crates/etr-config/    Shared config schema and validation
crates/etr-control/   Control-plane runtime and data-plane abstraction
crates/etrd/          Daemon entrypoint and management HTTP API
ebpf/etr-ebpf/        Aya-based TC ingress/egress programs
```

## Example Config

See [`config/etr.toml`](./config/etr.toml) for a point-to-point IPv4 forwarding example.

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
- `POST /api/v1/admin/reload` to reload the config file and re-apply rules

## Running The Control Plane

```bash
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

Current startup shape:

```bash
etrd --config config/etr.toml --bpf-object /path/to/etr-ebpf-object
```

Current implementation assumptions:

- attaches `etr_ingress` to TC ingress on `[data_plane].external_interface`
- attaches `etr_egress` to TC egress on the same interface
- syncs the `ETR_TC_FORWARD_RULES` and `ETR_TC_FLOW_STATE` maps from user-space config
- performs forward `DNAT` on ingress, forward `SNAT/MASQUERADE` on egress, and reverse NAT on ingress for backend replies

Linux forwarding prerequisites:

- `net.ipv4.ip_forward = 1`
- `net.ipv4.conf.<external_interface>.rp_filter = 0` or `2`
- upstream cloud security groups or host firewalls must allow the exposed frontend ports

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

## Deployment Notes

The current repository now contains:

- a Rust control plane that loads config, exposes reload APIs, and selects a data-plane backend
- shared map key/value types used by both user-space and eBPF code
- a Linux-only Aya TC backend loader in `etr-control`
- a first-pass TC ingress/egress eBPF implementation in `ebpf/etr-ebpf`

What is still pending is Linux-host validation of the eBPF build and runtime behavior.
