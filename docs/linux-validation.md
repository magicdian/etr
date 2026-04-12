# Linux Validation Runbook

This runbook is for real-host validation of the Linux TC dataplane after `etrd` is installed and running with the Aya-backed backend.

## Scope

Use this document to validate:

- startup preflight passes on the target host
- TCP forwarding works end to end
- UDP forwarding works end to end
- runtime counters and flow-state behavior match expectations

This is not a loopback validation guide.
`curl 127.0.0.1:<frontend_port>` is expected to fail because `etr` does not create a local listener on `lo`; it rewrites traffic that traverses the configured external interface.

## Preconditions

- `etrd` is installed and running
- `GET /api/v1/debug/dataplane` reports:
  - `backend = "tc-aya"`
  - `preflight.passed = true`
- TC programs are attached to the configured external interface
- target host networking baseline is already configured:
  - `net.ipv4.ip_forward = 1`
  - `net.ipv4.conf.<external_interface>.rp_filter = 0` or `2`
- security groups, ACLs, and host firewall allow the frontend ports

## Useful Commands

Check service health:

```bash
systemctl status etrd.service
curl http://127.0.0.1:9911/api/v1/debug/dataplane
```

Check TC attach state:

```bash
sudo tc filter show dev eth0 ingress
sudo tc filter show dev eth0 egress
```

Check BPF maps:

```bash
sudo bpftool map show
```

## Counter Meanings

`/api/v1/debug/dataplane` exposes these runtime counters:

- `ingress_rule_hits`: frontend packets matched a configured rule
- `ingress_reverse_hits`: backend reply packets matched reverse NAT on ingress
- `egress_flow_hits`: packets matched flow state on egress for SNAT/MASQUERADE
- `flow_creations`: new flow-state entries were inserted
- `rule_misses`: traffic traversed the interface but did not match a configured frontend rule
- `parse_drops`: malformed packets or parse failures caused drops

## TCP Validation

Assume:

- gateway public or interface IP: `81.71.89.210`
- frontend port: `18510`
- backend: `163.223.125.6:18510`
- external interface: `eth0`

### Step 1: Record baseline counters

```bash
curl http://127.0.0.1:9911/api/v1/debug/dataplane
```

### Step 2: Capture traffic on the gateway host

```bash
sudo tcpdump -ni eth0 'tcp port 18510'
```

### Step 3: From another machine, hit the frontend

```bash
curl -vk --connect-timeout 5 https://81.71.89.210:18510
```

### Step 4: Check expected outcomes

Good:

- client completes TCP/TLS handshake
- `ingress_rule_hits` increases
- `flow_creations` increases
- `flow_entries` becomes non-zero
- `egress_flow_hits` increases
- `ingress_reverse_hits` increases after backend replies

Bad:

- no packets on `tcpdump`
  - problem is before the host: wrong IP, security group, cloud NAT/LB, routing
- packets visible on `tcpdump` but `ingress_rule_hits = 0`
  - traffic is reaching the host but not matching the configured frontend rule
- `ingress_rule_hits` increases but client still hangs
  - forward match works; investigate backend reachability or return path

## UDP Validation

Assume:

- frontend port: `16020`
- backend: `163.223.125.6:11426`
- protocol: `udp`
- external interface: `eth0`

### Step 1: Confirm the rule is configured as UDP

```bash
grep -n -A8 'name = "ssh-proxy"' /etc/etr/etr.toml
```

### Step 2: Record baseline counters

```bash
curl http://127.0.0.1:9911/api/v1/debug/dataplane
```

### Step 3: Capture UDP traffic on the gateway host

```bash
sudo tcpdump -ni eth0 'udp port 16020 or udp port 11426'
```

### Step 4: Send UDP traffic from another machine

Use one of:

```bash
nc -u -v 81.71.89.210 16020
```

or:

```bash
socat - UDP:81.71.89.210:16020
```

If the backend protocol expects a specific payload, send a request that the backend can actually respond to.

### Step 5: Check expected outcomes

Good:

- frontend UDP packet appears on `eth0`
- `ingress_rule_hits` increases
- `flow_creations` increases
- `flow_entries` becomes non-zero
- if the backend responds, `ingress_reverse_hits` and `egress_flow_hits` increase as well

Bad:

- no frontend UDP packet on `eth0`
  - problem is before the gateway host
- `rule_misses` increases but `ingress_rule_hits` does not
  - UDP packet reached the interface but did not match the configured rule
- `ingress_rule_hits` increases but there is no backend response
  - investigate backend UDP service behavior, cloud egress policy, or return path

## Backend Reachability Checks

From the gateway host, test whether the configured backend itself is reachable:

TCP:

```bash
nc -vz 163.223.125.6 18510
```

UDP:

```bash
nc -u -v 163.223.125.6 11426
```

These checks help separate gateway forwarding issues from backend availability issues.

## Minimal Validation Record

For each environment, capture at least:

- frontend IP and port used for validation
- external interface name
- before/after `/api/v1/debug/dataplane` output
- whether TCP succeeded end to end
- whether UDP succeeded end to end
- a short note if security-group or routing issues were discovered outside the host
