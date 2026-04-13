# Journal - magicdian (Part 1)

> AI development session journal
> Started: 2026-04-12

---



## Session 1: Linux TC NAT validation and forwarding fixes

**Date**: 2026-04-12
**Task**: Linux TC NAT validation and forwarding fixes

### Summary

(Add summary)

### Main Changes

| Area | Description |
|------|-------------|
| Data plane | Converted `etr-ebpf` into a loadable binary target and validated Linux TC attachment on Debian 6.1. |
| NAT behavior | Fixed forward NAT ordering to mirror `PREROUTING DNAT` plus `POSTROUTING MASQUERADE`, and split reverse NAT so backend replies are translated back to the client successfully. |
| Encoding | Fixed IPv4 address encoding for BPF map values on `bpfel` so backend addresses are no longer byte-swapped in live traffic. |
| Runtime validation | Confirmed `bpftool` rule and flow maps populated correctly, verified external SYN forwarding and translated SYN-ACK return path with `tcpdump`, and completed end-to-end `curl` validation from the client. |
| Knowledge capture | Added Linux TC data-plane code-spec covering host prerequisites, hook timing, map contracts, validation matrix, and common debugging mistakes. |

**Updated Files**:
- `ebpf/etr-ebpf/src/main.rs`
- `crates/etr-control/src/kernel.rs`
- `README.md`
- `docs/architecture.md`
- `.trellis/spec/backend/linux-tc-dataplane.md`
- `.trellis/spec/backend/index.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`


### Git Commits

| Hash | Message |
|------|---------|
| `edd4461` | (see git log) |
| `9157b57` | (see git log) |
| `4d63569` | (see git log) |
| `56eccac` | (see git log) |
| `8af558b` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 2: Sanitize tracked config and rewrite history

**Date**: 2026-04-12
**Task**: Sanitize tracked config and rewrite history

### Summary

Replaced the tracked runtime config with config/etr.toml.example, sanitized backend IPs to 8.8.8.8, added config/etr.toml to .gitignore, updated docs, guided git-filter-repo history cleanup, restored origin, and verified dev matches origin/dev after the force-push flow.

### Main Changes

(Add details)

### Git Commits

| Hash | Message |
|------|---------|
| `6cb54a5` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 3: Release Packaging And Linux Preflight

**Date**: 2026-04-13
**Task**: Release Packaging And Linux Preflight

### Summary

(Add summary)

### Main Changes

| Area | Description |
|------|-------------|
| Release workflow | Added workspace-version bump tooling and versioned Linux release bundle generation |
| Installation | Added canonical `etrd install` / `etrd uninstall` flow with `systemd` first and init.d fallback |
| Runtime safety | Added Linux startup preflight checks for root privileges, BTF, object path, interface state, `ip_forward`, and `rp_filter` |
| Observability | Added JSON dataplane debug/status surfaces plus runtime counters for rule hits, misses, flow creation, and parse drops |
| Docs/spec | Updated README and Linux TC backend code-spec to document release layout, install flow, sysctl baseline, and loopback-vs-interface behavior |
| Validation | Verified formatting, `cargo check`, `cargo test`, release artifact build, `systemd` service startup, and TCP end-to-end forwarding on Linux |

**Residual Work**:
- UDP real-host validation is still pending
- Linux integration coverage remains lighter than the desired v1 bar
- Task remains active for the next implementation slice


### Git Commits

| Hash | Message |
|------|---------|
| `0d32ee73bf6fbcba7d2c98ea6802b733e1b8f09d` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete


## Session 4: Linux Validation Runbook And UDP Verification

**Date**: 2026-04-13
**Task**: Linux Validation Runbook And UDP Verification

### Summary

(Add summary)

### Main Changes

| Area | Description |
|------|-------------|
| Validation docs | Added a Linux real-host validation runbook for TCP and UDP forwarding, including expected dataplane counters, packet-capture checks, and backend reachability checks |
| Code-spec update | Extended the Linux TC code-spec with executable guidance for loopback-vs-interface validation, wrong-public-IP troubleshooting, and UDP success signals |
| Manual verification | Confirmed that TCP end-to-end forwarding works against the real frontend IP and that UDP traffic increments rule-hit, flow-creation, reverse-hit, and egress-flow counters on Linux |
| README sync | Linked the new runbook from README and clarified that `127.0.0.1:<frontend_port>` is not a valid validation path |

**Residual Work**:
- Automated Linux integration coverage is still lighter than the original productization goal
- Task remains active until that gap is either implemented or explicitly deferred


### Git Commits

| Hash | Message |
|------|---------|
| `849f4d980e99032f4fabe472d0a9d216ba321998` | (see git log) |

### Testing

- [OK] (Add test results)

### Status

[OK] **Completed**

### Next Steps

- None - task complete
