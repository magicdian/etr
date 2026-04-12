# Linux TC Data Plane

> Executable code-spec for the current Linux TC forwarding path in `etr`.

## Scenario: IPv4 Frontend Port Forwarding With Remote Backend

### 1. Scope / Trigger
- Trigger: any change to `crates/etr-control/src/linux.rs`, `crates/etr-control/src/kernel.rs`, `crates/etr-types/src/lib.rs`, or `ebpf/etr-ebpf/src/main.rs`
- Trigger: any change to Linux forwarding semantics, TC attach points, flow-state map layout, or operator prerequisites
- Scope: single-host Linux gateway, IPv4 only, TCP/UDP only, one frontend maps to one backend

### 2. Signatures
```rust
pub fn encode_enabled_rules(config: &EtrConfig) -> Vec<EncodedRule>

pub const TC_FORWARD_RULES_MAP: &str = "ETR_TC_FORWARD_RULES";
pub const TC_FLOW_STATE_MAP: &str = "ETR_TC_FLOW_STATE";
pub const TC_RUNTIME_STATS_MAP: &str = "ETR_TC_RUNTIME_STATS";
pub const TC_INGRESS_PROGRAM_NAME: &str = "etr_ingress";
pub const TC_EGRESS_PROGRAM_NAME: &str = "etr_egress";
```

Runtime startup:

```bash
etrd --config config/etr.toml --bpf-object /path/to/etr-ebpf
```

Override startup preflight only when intentionally accepting degraded bring-up:

```bash
etrd --config config/etr.toml --bpf-object /path/to/etr-ebpf --allow-preflight-warnings
```

Linux eBPF build:

```bash
RUSTFLAGS="-C debuginfo=2 -C link-arg=--btf" \
CARGO_TARGET_BPFEL_UNKNOWN_NONE_LINKER=bpf-linker \
cargo +nightly build -p etr-ebpf \
  --target bpfel-unknown-none \
  -Z build-std=core \
  --release
```

### 3. Contracts

Forward rule map contract:

```rust
#[repr(C)]
pub struct ForwardRuleKey {
    pub protocol: u8,
    pub reserved: [u8; 3],
    pub listen_addr_be: u32,
    pub listen_port_be: u16,
    pub padding: u16,
}

#[repr(C)]
pub struct ForwardRuleValue {
    pub backend_addr_be: u32,
    pub backend_port_be: u16,
    pub snat_mode: u8,
    pub reserved: u8,
}
```

Flow state contract:

```rust
#[repr(C)]
pub struct FlowStateKey {
    pub protocol: u8,
    pub reserved: [u8; 3],
    pub src_addr_be: u32,
    pub dst_addr_be: u32,
    pub src_port_be: u16,
    pub dst_port_be: u16,
}

#[repr(C)]
pub struct FlowStateValue {
    pub rewrite_src_addr_be: u32,
    pub rewrite_src_port_be: u16,
    pub rewrite_dst_addr_be: u32,
    pub rewrite_dst_port_be: u16,
    pub snat_mode: u8,
    pub reserved: [u8; 3],
}
```

Traffic ordering contract:

- `etr_ingress` handles forward `DNAT`
- `etr_egress` handles forward `SNAT/MASQUERADE`
- `etr_ingress` handles reverse destination restoration for backend replies
- `etr_egress` handles reverse source restoration for backend replies

Operator prerequisites:

- `net.ipv4.ip_forward = 1`
- `net.ipv4.conf.<external_interface>.rp_filter = 0` or `2`
- `/sys/kernel/btf/vmlinux` must be present
- process must run with privileges sufficient for TC attach and BPF map access
- `[data_plane].external_interface` must exist and be operational
- cloud security groups and host firewalls must allow the configured frontend ports

Observability contract:

- `/api/v1/status` returns runtime snapshot including data-plane status
- `/api/v1/debug/dataplane` returns backend identity, installed rules, active flow count, preflight report, and runtime counter summary
- runtime counter summary includes ingress rule hits, ingress reverse hits, egress flow hits, flow creations, rule misses, and parse drops

Encoding contract:

- IPv4 addresses written into BPF maps must use `u32::from_ne_bytes(addr.octets())`
- Do not use `u32::from_be_bytes` for map encoding on the current `bpfel` target
- TCP/UDP ports remain encoded with `to_be()`

### 4. Validation & Error Matrix

| Condition | Boundary | Expected Behavior |
|----------|----------|-------------------|
| Linux startup without `--bpf-object` | `etr-control` | fall back to `tc-stub` with warning |
| Missing TC program in object | Linux loader | return `DataPlaneError::Setup` |
| Missing map in object | Linux loader | return `DataPlaneError::MapSync` |
| Linux startup preflight fails | Linux loader | return `DataPlaneError::Preflight` unless override is enabled |
| IPv6 rule in config | `etr-config` | reject config validation |
| Multi-backend rule in MVP | `etr-config` | reject config validation |
| `ip_forward = 0` | host runtime | packets may match eBPF but will not forward successfully |
| strict `rp_filter = 1` | host runtime | NAT/forwarded packets may be dropped |
| missing `/sys/kernel/btf/vmlinux` | host runtime | startup fails before dataplane activation |
| wrong IPv4 map byte order | control/kernel boundary | backend address appears byte-swapped in packet capture |
| forward SNAT applied at ingress | TC program ordering | flow state grows but end-to-end connection fails |

### 5. Good / Base / Bad Cases

Good:

- external SYN to `81.71.89.210:18510`
- ingress capture shows `116.28.x.x:* -> 10.1.0.10:18510`
- egress capture shows `10.1.0.10:* -> 163.223.125.6:18510`
- backend SYN-ACK returns
- client sees connection complete
- `/api/v1/debug/dataplane` shows passing preflight checks and non-zero counters after traffic
- UDP traffic through a configured rule also creates flow state and increments counters

Base:

- `etrd` runs on Linux with `--bpf-object`
- preflight passes or operator explicitly enabled `--allow-preflight-warnings`
- `bpftool map show` lists the forward rule map, flow-state map, and runtime stats map
- `bpftool map dump` shows configured frontend rules

Bad:

- no packets visible on `tcpdump -ni any 'tcp port <frontend>'`
- flow-state map remains empty after external SYN
- debug endpoint reports failed critical preflight checks while operator expected healthy startup
- forwarded packets show `6.125.223.163` instead of `163.223.125.6`
- backend SYN-ACK reaches the gateway but no translated response leaves toward the client

### 6. Tests Required

- `cargo fmt --all`
- `cargo check`
- `cargo test`
- unit test in `crates/etr-control/src/kernel.rs` asserting non-zero IPv4 addresses are encoded with packet-byte layout
- Linux manual validation:
  - start `etrd` with `--bpf-object`
  - confirm `tc filter show dev <if> ingress` and `egress` show `etr_ingress` and `etr_egress`
  - confirm `bpftool map dump` shows rule entries
  - confirm `/api/v1/debug/dataplane` reports preflight state and runtime counters
  - confirm both TCP and UDP traffic exercise the forwarding path
  - confirm packet capture shows forward SYN to backend and translated reply back to client

Assertion points for Linux manual validation:

- forward SYN leaves with backend IP and original client source port
- backend reply is translated back to frontend listener identity before leaving the host
- flow-state map grows when external traffic hits a configured rule

### 7. Wrong vs Correct

#### Wrong

Apply forward `SNAT/MASQUERADE` at TC ingress because it looks equivalent to netfilter NAT:

```rust
// Wrong: source rewrite happens before the packet has traversed the host
// forwarding path.
rewrite_ipv4_addr(&mut ctx, saddr_offset, ip_check, l4_check, client_ip, frontend_ip)?;
rewrite_ipv4_addr(&mut ctx, daddr_offset, ip_check, l4_check, frontend_ip, backend_ip)?;
```

Why it fails:

- the packet no longer reflects the real ingress source while still on ingress
- Linux forwarding and return handling can break even though rule and flow maps look correct

#### Correct

Keep NAT ordering aligned with `iptables` semantics:

```rust
// Ingress: forward DNAT only
rewrite_ipv4_addr(&mut ctx, daddr_offset, ip_check, l4_check, frontend_ip, backend_ip)?;

// Egress: forward SNAT/MASQUERADE
rewrite_ipv4_addr(&mut ctx, saddr_offset, ip_check, l4_check, client_ip, frontend_ip)?;
```

Also encode IPv4 map addresses using packet-byte layout:

```rust
let encoded = u32::from_ne_bytes(addr.octets());
```

## Common Mistakes

### Common Mistake: Assuming eBPF attach success means forwarding works

**Symptom**: `etrd` reports `tc-aya`, TC filters are attached, but client connections hang.

**Cause**: attach success only proves the program loaded. It does not prove host sysctls, security groups, NAT ordering, or map encoding are correct.

**Fix**:

- verify `ip_forward` and `rp_filter`
- inspect rule and flow maps with `bpftool`
- confirm packet path with `tcpdump`

### Common Mistake: Reading full BPF map names from `bpftool`

**Symptom**: `bpftool map dump name ETR_TC_FORWARD_RULES` fails with `can't parse name`.

**Cause**: kernel BPF object names are truncated.

**Fix**:

- use `bpftool map show` first
- dump by `id` when in doubt
