# etr Architecture Notes

## Current Split

- `etr-types`: shared wire types for TC maps and flow state
- `etrd`: daemon process, config loading, validation, reload API
- `etr-control`: runtime state and data-plane abstraction
- `etr-config`: shared config contract and validation rules
- `etr-ebpf`: Aya-based TC ingress/egress programs

## Current MVP Contract

- One rule maps one frontend listener to one backend `IP:port`
- TCP/UDP only
- IPv4 only
- SNAT/MASQUERADE return path
- Single-host gateway deployment

## Current Linux TC Flow

- ingress:
  match frontend rule and apply forward `DNAT`
- egress:
  apply forward `SNAT/MASQUERADE` for packets leaving toward the backend
- ingress:
  apply reverse NAT for backend replies before they re-enter the host stack

This ordering intentionally mirrors the operational shape of `iptables` `PREROUTING DNAT` plus
`POSTROUTING MASQUERADE`, because applying source NAT too early at ingress can break forwarding on
real gateway hosts.

## Linux Host Prerequisites

- `net.ipv4.ip_forward = 1`
- `rp_filter` on the external interface should be `0` or `2`
- cloud security groups and host firewalls must allow the configured frontend ports

## Planned Evolution

- Add IPv6 parsing and rule validation
- Add XDP backend without changing the control-plane rule model
- Add multi-backend selection and health checks later
 - Add Linux build tooling that packages the eBPF object with the daemon release
