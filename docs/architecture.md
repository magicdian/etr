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

## Planned Evolution

- Add IPv6 parsing and rule validation
- Add XDP backend without changing the control-plane rule model
- Add multi-backend selection and health checks later
 - Add Linux build tooling that packages the eBPF object with the daemon release
