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
| client tests `127.0.0.1:<frontend_port>` | validation workflow | request bypasses the external interface path and is expected to fail with local connection refusal |
| `ip_forward = 0` | host runtime | packets may match eBPF but will not forward successfully |
| strict `rp_filter = 1` | host runtime | NAT/forwarded packets may be dropped |
| missing `/sys/kernel/btf/vmlinux` | host runtime | startup fails before dataplane activation |
| public IP or upstream route is wrong | host ingress | `tcpdump` on the external interface shows no packets and dataplane rule-hit counters do not move |
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
- successful UDP validation increments `ingress_rule_hits`, `flow_creations`, and, when the backend responds, also increments `ingress_reverse_hits` and `egress_flow_hits`

Base:

- `etrd` runs on Linux with `--bpf-object`
- preflight passes or operator explicitly enabled `--allow-preflight-warnings`
- `bpftool map show` lists the forward rule map, flow-state map, and runtime stats map
- `bpftool map dump` shows configured frontend rules

Bad:

- `curl 127.0.0.1:<frontend_port>` returns `Connection refused`
- no packets visible on `tcpdump -ni any 'tcp port <frontend>'`
- flow-state map remains empty after external SYN
- debug endpoint reports failed critical preflight checks while operator expected healthy startup
- external clients hit the wrong public IP, `tcpdump` shows no frontend traffic, and only unrelated `rule_misses` continue to rise
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
- loopback access to `127.0.0.1:<frontend_port>` is not used as a validation signal because it does not exercise TC forwarding on the external interface

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

#### Wrong

Validate a frontend rule using loopback or treat a successful service startup as proof that the frontend is reachable:

```bash
curl -vk https://127.0.0.1:18510
systemctl status etrd.service
```

Why it fails:

- `etr` is not a local TCP/UDP listener on `lo`
- service health only proves bootstrap, not that external-interface traffic is reaching the host

#### Correct

Validate from another machine against the host's actual interface or public IP and correlate with dataplane counters:

```bash
curl -vk --connect-timeout 5 https://81.71.89.210:18510
curl http://127.0.0.1:9911/api/v1/debug/dataplane
sudo tcpdump -ni eth0 'tcp port 18510'
```

## Common Mistakes

### Common Mistake: Assuming eBPF attach success means forwarding works

**Symptom**: `etrd` reports `tc-aya`, TC filters are attached, but client connections hang.

**Cause**: attach success only proves the program loaded. It does not prove host sysctls, security groups, NAT ordering, or map encoding are correct.

**Fix**:

- verify `ip_forward` and `rp_filter`
- inspect rule and flow maps with `bpftool`
- confirm packet path with `tcpdump`

### Common Mistake: Using `127.0.0.1` to validate frontend reachability

**Symptom**: `curl 127.0.0.1:<frontend_port>` returns `Connection refused`, so it looks like the frontend rule is broken.

**Cause**: the frontend port is implemented as TC forwarding on the configured external interface, not as a local loopback listener.

**Fix**:

- validate from another host against the real interface IP or public IP
- correlate the test with `/api/v1/debug/dataplane` counters
- use `tcpdump` on the external interface, not just `lo`

### Common Mistake: Chasing dataplane logic when the wrong public IP is being tested

**Symptom**: external clients hang, but the gateway host sees no matching frontend traffic and only unrelated `rule_misses` increase.

**Cause**: traffic is not reaching the host at all because the public IP, cloud NAT/LB path, or upstream routing is wrong.

**Fix**:

- confirm the tested public IP actually maps to the gateway host
- capture on `tcpdump -ni <if> 'tcp port <frontend> or udp port <frontend>'`
- if no packets arrive, debug the cloud/network path before changing `etr`

### Common Mistake: Reading full BPF map names from `bpftool`

**Symptom**: `bpftool map dump name ETR_TC_FORWARD_RULES` fails with `can't parse name`.

**Cause**: kernel BPF object names are truncated.

**Fix**:

- use `bpftool map show` first
- dump by `id` when in doubt
