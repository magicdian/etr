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
