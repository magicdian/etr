# etr MVP Routing Service

## Goal

Build `etr`, an eBPF-based transit routing service that forwards traffic received on a local machine port or interface to a target IP and port with better performance and simpler rule management than manually maintaining `iptables` DNAT/SNAT rules.

## What I already know

* Product name: `etr`
* The project is currently empty except for Trellis scaffolding and a minimal README
* The product is intended to be an eBPF-based routing / forwarding service
* The user prefers Rust for implementation because of memory safety and compile-time guarantees, but Go is also acceptable if it is a better fit
* Core capability: configure forwarding rules
* Preferred protocol coverage includes TCP / UDP / ICMP
* Existing manual workflow uses `iptables` rules for `PREROUTING`, `FORWARD`, and `POSTROUTING`
* The desired behavior is: forward traffic arriving at local port A to destination IP port B
* Rule management may be either configuration-file based or dynamic via a Web UI
* The user is interested in replacing `iptables`-based forwarding with eBPF at XDP or TC hook points for higher performance

## Assumptions (temporary)

* MVP can start with one management plane only, rather than both config file and Web UI
* MVP may need to support only a subset of protocol and hook combinations first, then expand
* Because ICMP forwarding behaves differently from TCP/UDP port forwarding, its MVP behavior should be deferred until the L4 forwarding path is stable
* A production-ready implementation will likely need both traffic redirection logic and connection/state handling considerations

## Open Questions

* None for MVP requirement discovery at the moment

## Requirements (evolving)

* Provide a way to define forwarding rules without manually editing `iptables`
* Support forwarding traffic that arrives on the local machine to a configured target IP and target port
* Preserve a path to implement the forwarding plane with eBPF rather than `iptables`
* Support `TCP` and `UDP` in the MVP
* Keep the product extensible enough to add `ICMP` later
* Keep configuration management simple for the MVP
* Use a hybrid architecture that can support multiple eBPF hook backends over time
* Implement the first data plane on `TC`
* Use a static configuration file as the source of truth in user space
* Provide an HTTP reload API so rule changes can be applied without editing kernel attachments manually
* Package the product for deployment as a prebuilt binary and eBPF artifact, with portability strategy based on `BTF` / `CO-RE`
* Officially support Linux kernel `5.15` and newer for the MVP
* Support IPv4 forwarding in the MVP
* Design the rule model and internal abstractions so IPv6 can be added later without redesigning the control plane
* Handle return traffic in the MVP with SNAT/MASQUERADE-style behavior by default
* Deliver the MVP as a single-host gateway daemon
* Keep configuration and internal interfaces extensible enough that a future multi-node control plane is still possible
* Each forwarding rule maps to a single backend target in the MVP
* The config model and internal rule representation should leave room for future multi-backend load balancing

## Acceptance Criteria (evolving)

* [ ] A developer can define at least one forwarding rule in a supported configuration format
* [ ] Traffic sent to a configured local destination is forwarded to the configured backend destination
* [ ] The architecture clearly identifies where eBPF is used and what remains in user space
* [ ] The MVP scope explicitly defines that `TCP` and `UDP` are supported now and `ICMP` is deferred
* [ ] The implementation plan identifies the initial control plane and data plane responsibilities
* [ ] The architecture leaves room for a future `XDP` backend without redesigning the rule model
* [ ] Rule changes can be reloaded from the config file through an HTTP API
* [ ] Deployment assumptions clearly state what kernel-side `BTF` / `CO-RE` support is required
* [ ] The supported deployment baseline clearly documents Linux kernel `5.15+`
* [ ] The initial rule and config model can be extended to IPv6 later without breaking changes
* [ ] The first forwarding path supports NATed return traffic for backends that cannot route directly back to the original client IP
* [ ] The MVP can run as a single-host gateway without requiring a central controller
* [ ] The config model can be extended to multiple backends later without breaking the single-backend MVP contract

## Definition of Done (team quality bar)

* Tests added or updated where appropriate
* Lint, typecheck, and build checks pass
* Docs are updated for behavior and configuration
* Risks and rollout constraints are documented for networking behavior

## Out of Scope (explicit)

* `ICMP` support in the first MVP
* A full production-grade distributed control plane
* Automatic migration of existing `iptables` rules into etr
* Full multi-tenant rule isolation
* High-availability clustering for the first MVP

## Research Notes

### What official sources indicate

* The Linux kernel `XDP_REDIRECT` path is built around redirecting frames through `DEVMAP`, `CPUMAP`, or `XSKMAP`, and the kernel documentation notes that support varies by driver and packet shape.
* Kernel documentation for `SOCKMAP` and `SOCKHASH` shows socket-level redirection options, but those are conceptually different from general-purpose host port-forwarding to arbitrary backend `IP:port` targets.
* Aya provides a pure-Rust eBPF toolchain that does not require `libbpf` or a C toolchain, which makes it attractive for an all-Rust MVP.
* `cilium/ebpf` provides a mature pure-Go userspace library, but the common workflow still centers on loading compiled eBPF objects and managing attachments from Go.

### Constraints for this project

* The product goal is closer to configurable host forwarding / NAT-like behavior than to pure NIC-to-NIC frame redirection
* The repo is empty, so maintainability and delivery speed matter alongside raw packet-path performance
* The user prefers Rust unless Go is materially better for this use case
* The user selected a hybrid hook strategy with `TC` as the initial implementation target
* The user selected `static config file + HTTP reload API` as the MVP control plane
* The user selected Linux kernel `5.15` as the minimum supported baseline because one target server is `5.15.0-117-generic`
* The user selected an IPv4-first MVP while expecting the architecture and config model to leave room for future IPv6 support
* The user selected SNAT/MASQUERADE-style return-path handling for MVP because backends may not be able to route directly to original client IPs
* The user selected a single-host gateway MVP, while expecting the design not to block a future multi-node control plane
* The user selected a point-to-point single-backend rule model for MVP, while expecting the config format to leave room for future load balancing

### Feasible approaches here

**Approach A: TC-first eBPF data plane with Rust control plane** (Recommended)

* How it works:
  * Attach eBPF programs at TC ingress/egress
  * Use maps to store forwarding rules and connection metadata
  * Keep rule loading, config parsing, and lifecycle management in Rust user space
* Pros:
  * Better fit for packet rewriting and host forwarding than raw XDP-only redirection
  * Lower operational risk than relying on XDP redirect behavior first
  * Aligns with the preferred Rust implementation path
* Cons:
  * Slightly less headline performance than a pure XDP-first design
  * Still requires careful connection/state design for return traffic

**Approach B: XDP-first fast path with Rust control plane**

* How it works:
  * Attach at XDP for earliest packet interception
  * Implement redirect and rewrite logic as much as possible at XDP
  * Use user space to manage rule maps and lifecycle
* Pros:
  * Maximum packet-path performance potential
  * Strong future story for very high throughput
* Cons:
  * Higher implementation complexity for NAT-like forwarding
  * Driver support and redirect limitations are a bigger operational risk
  * Harder MVP path for a brand-new project

**Approach C: Hybrid staged design**

* How it works:
  * Architect the control plane and rule model to be hook-agnostic
  * Implement MVP on TC first
  * Leave a later XDP fast path as a second execution backend
* Pros:
  * Best balance of delivery risk and future extensibility
  * Avoids locking the product to one hook strategy too early
* Cons:
  * Slightly more upfront architecture work than a single-path prototype
  * Requires discipline to keep abstractions clean

### BTF / CO-RE portability notes

* Kernel documentation states that BPF CO-RE combines compiler-generated relocation metadata with runtime kernel `BTF` so one BPF build can adapt across different kernel versions and configurations.
* The same kernel documentation states that portability relies on the running kernel exposing authoritative `BTF` at `/sys/kernel/btf/vmlinux`.
* Aya states that, with `BTF` support and a musl-linked userspace binary, it can provide a practical compile-once-run-on-many-Linux-distributions workflow.
* Inference for this project:
  * `BTF` is not a magic packaging feature by itself; it is part of the portability mechanism
  * For "compile once, ship to many hosts" we should design around `CO-RE` and require target kernels that expose usable `vmlinux BTF`
  * If some target hosts do not provide compatible kernel `BTF` or required eBPF features, we will need either a stricter supported-kernel matrix or fallback build/distribution strategies

## Decision (ADR-lite)

**Context**: The product needs an eBPF data plane that can eventually pursue very high performance, but the first version also needs to be realistic to deliver for a new empty project.

**Decision**: Use a hybrid architecture for the forwarding data plane, with `TC` as the first implemented backend and room for a future `XDP` backend.

**Consequences**:

* MVP implementation should define rule abstractions and control-plane interfaces that are not tightly coupled to `TC`
* The first shipped path prioritizes delivery safety and packet rewrite flexibility over absolute fastest-path marketing numbers
* Future `XDP` work should be additive rather than a rewrite

**Context**: The product needs simple but practical rule management for MVP without turning the first version into a full management platform.

**Decision**: Use a static configuration file as the source of truth, and add an HTTP reload API for applying updated rules.

**Consequences**:

* The first version can stay focused on forwarding behavior rather than UI development
* Config parsing, validation, and map updates become a central responsibility of the Rust userspace daemon
* A later Web UI can be built on top of the same reload and config-management workflow

**Context**: The product needs a realistic kernel support floor that matches an actual deployment target while still enabling a practical `BTF` / `CO-RE` strategy.

**Decision**: Officially support Linux kernel `5.15+` for the MVP.

**Consequences**:

* Feature design must stay within the eBPF capabilities we can rely on in `5.15`
* Documentation should clearly state that prebuilt artifacts assume compatible kernel support and accessible `vmlinux BTF`
* Hosts that do not meet those assumptions may require alternate packaging or local builds, but are outside the default support path

**Context**: Current real-world usage is IPv4, but the product should not paint itself into an IPv4-only corner.

**Decision**: Support IPv4 only in the MVP, while designing the rule model and control-plane abstractions to allow future IPv6 support.

**Consequences**:

* MVP delivery stays focused on the primary real-world deployment scenario
* Address-family handling must be represented explicitly in internal models even if only IPv4 is enabled at first
* IPv6 packet parsing, checksum handling, and routing behavior are deferred rather than ignored

**Context**: Real deployment backends may not be able to route traffic directly back to original client IPs, so symmetric return-path handling matters more than preserving source identity in the first version.

**Decision**: Use SNAT/MASQUERADE-style return-path handling by default in the MVP instead of source-IP preservation.

**Consequences**:

* The first version should optimize for reliable end-to-end forwarding rather than backend visibility of real client source IPs
* Flow state and reverse-path handling become explicit parts of the TC data-plane design
* Source-IP preservation can be explored later as an advanced mode with stronger topology requirements

**Context**: The first version should be practical to deploy on one forwarding host, but the product should still be able to grow into a broader managed routing platform later.

**Decision**: Build the MVP as a single-host gateway daemon, while keeping config structures and internal APIs extensible enough for a future multi-node control plane.

**Consequences**:

* The first version can avoid distributed coordination, agent registration, and central state management
* Rule ownership and lifecycle remain local to one host in MVP
* Naming and config layout should avoid assuming there is only ever one node forever

**Context**: The primary real-world use case is point-to-point port forwarding, but future product evolution may include multiple backends per frontend listener.

**Decision**: In the MVP, each forwarding rule maps to exactly one backend target. The config model and internal rule representation should still leave room for future multi-backend load balancing.

**Consequences**:

* The first version can keep lookup logic, flow state, and observability simple
* The config schema should avoid dead-end field names that make multi-backend expansion awkward later
* Load-balancing policy, health checking, and backend selection algorithms are explicitly deferred

## Technical Notes

* Existing repo state:
  * `README.md` only contains the project name and a one-line description
  * No application source code exists yet
* Existing manual example from the user:
  * TCP DNAT from local ports `16020` and `18510` to backend `163.223.125.6` ports `11426` and `18510`
  * Corresponding `FORWARD` and `POSTROUTING MASQUERADE` rules are required today
* Likely future design areas to clarify:
  * Language/runtime choice: Rust vs Go
  * ICMP semantics and whether it means forwarding echo traffic, policy routing, or generic pass-through
  * Exact config schema and reload semantics
* Sources:
  * Linux kernel redirect docs: https://docs.kernel.org/bpf/redirect.html
  * Linux kernel sockmap docs: https://docs.kernel.org/bpf/map_sockmap.html
  * Linux kernel libbpf overview: https://docs.kernel.org/bpf/libbpf/libbpf_overview.html
  * Linux kernel BTF docs: https://docs.kernel.org/bpf/btf.html
  * Linux kernel CO-RE relocation docs: https://docs.kernel.org/bpf/llvm_reloc.html
  * Aya README: https://github.com/aya-rs/aya
  * cilium/ebpf README: https://github.com/cilium/ebpf
