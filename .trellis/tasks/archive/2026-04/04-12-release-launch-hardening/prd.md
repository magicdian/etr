# brainstorm: release and launch hardening

## Goal

Close the main productization gaps between the current MVP implementation and a release-ready `etr` Linux deliverable. This includes turning manual build steps into a releasable package flow, making startup preflight checks actionable, validating Linux forwarding claims more rigorously, strengthening observability, improving Linux integration testing, and aligning docs with the actual shipped state.

## What I already know

* The repository currently supports manual builds for `etrd` and the `etr-ebpf` object, but there is no one-command release packaging workflow yet.
* The archived MVP task already captured a requirement to package the product as prebuilt binary and eBPF artifact, with portability assumptions based on `BTF` / `CO-RE`.
* [`README.md`](/home/github_projects/etr/README.md) documents manual eBPF build steps and daemon startup, but does not describe a versioned release artifact flow, installer, or systemd unit integration.
* [`README.md`](/home/github_projects/etr/README.md) still says Linux-host validation of the eBPF build/runtime is pending, which appears inconsistent with current repo state.
* [`crates/etr-control/src/linux.rs`](/home/github_projects/etr/crates/etr-control/src/linux.rs) loads the eBPF object, attaches TC ingress/egress programs, and syncs rule/flow maps. It does not productize preflight host checks such as `ip_forward`, `rp_filter`, interface readiness, or kernel `BTF` diagnostics.
* [`crates/etr-config/src/lib.rs`](/home/github_projects/etr/crates/etr-config/src/lib.rs) allows both `tcp` and `udp` protocols at the config layer.
* Current management API surface is limited to `healthz`, config view, and reload; there is no visible metrics/debug surface for flow counts, hit counters, or forwarding failures in the README.
* Backend guidance already calls out Linux TC prerequisites such as `ip_forward` and `rp_filter`, and suggests integration-style tests will become necessary as TC wiring grows more complex.

## Assumptions (temporary)

* This should likely be split into multiple implementation slices rather than one large PR.
* The task is primarily backend/Linux productization work; no frontend changes are expected.
* "Release-ready" probably means a reproducible local/CI build that emits a versioned Linux bundle plus install/runtime assets, not necessarily distro-native packages yet.
* The user prefers the broadest scope among the previously proposed options: release packaging + startup preflight checks + UDP/Linux validation + observability improvements.

## Open Questions

* None currently. Ready for implementation confirmation.

## Requirements (evolving)

* Add a productized release artifact workflow for Linux deployment.
* Replace manual startup prerequisite discovery with explicit runtime preflight diagnostics.
* Reconcile product claims around UDP support and Linux validation with actual tested coverage.
* Improve observability enough that common forwarding issues can be diagnosed without relying entirely on `bpftool` and `tcpdump`.
* Strengthen Linux integration testing around NAT and real forwarding behavior.
* Update documentation to match the real state of the project.
* Support a lightweight deployment path where operators can distribute a binary bundle and run `etrd install` / `etrd uninstall` to install or remove the service manager integration.
* Prefer service-manager auto-detection with `systemd` first and `/etc/init.d` fallback when `systemd` is unavailable.
* Keep open the option to also ship an external installer flow, provided it does not fork the service installation logic.
* Treat `etrd install` / `etrd uninstall` as the canonical installation and removal contract.
* If an external installer script is shipped, it must remain a thin wrapper over the same canonical binary install logic.
* Ship a versioned Linux release bundle following an FHS-style installation layout.
* Default install paths should be:
  * binary: `/usr/local/bin/etrd`
  * eBPF object: `/usr/local/lib/etr/etr-ebpf`
  * config: `/etc/etr/etr.toml`
  * state/runtime data: `/var/lib/etr`
  * service definition: `/etc/systemd/system/etrd.service` or `/etc/init.d/etrd`
* Use `Cargo.toml` workspace version as the single source of truth for product versioning.
* Adopt the version format `YYMM.D.BUILD`, for example `2604.12.1` or `2604.9.1`.
* Add a version bump tool that updates the workspace version using the current date:
  * if the existing version date matches today, increment `BUILD`
  * if the existing version date does not match today, set the version to today's `YYMM.D.1`
* Release artifact naming must derive from the same canonical version source.
* Startup preflight checks must fail daemon startup by default when critical prerequisites are not met.
* Allow an explicit override path for degraded bring-up when operators intentionally want to bypass preflight failures.

## Acceptance Criteria (evolving)

* [ ] There is an agreed MVP scope for this task, with explicit in-scope and out-of-scope items.
* [ ] Release packaging expectations are defined concretely enough to implement and verify.
* [ ] Startup preflight behavior is defined concretely enough to implement and verify.
* [ ] Validation/test expectations for Linux TCP/UDP behavior are defined concretely enough to implement and verify.
* [ ] Documentation updates are tied to the selected scope instead of remaining as cleanup notes.
* [ ] Installation behavior is defined concretely enough to implement and verify across `systemd` and fallback init systems.
* [ ] Release bundle layout and installation paths are defined concretely enough to implement and verify.
* [ ] The release artifact naming and default filesystem layout are defined concretely enough to implement and verify.
* [ ] Versioning rules and bump workflow are defined concretely enough to implement and verify.
* [ ] Critical preflight failures stop startup by default and report actionable diagnostics.
* [ ] An explicit override path exists for intentional degraded bring-up.

## Definition of Done (team quality bar)

* Tests added or updated where appropriate
* Lint / typecheck / CI green
* Docs and operator-facing notes updated if behavior changes
* Rollout and failure modes considered for risky runtime changes

## Out of Scope (explicit)

* Frontend or Web UI work
* Non-Linux data-plane work unless required by the selected MVP scope
* Distro-specific package formats such as `.deb` / `.rpm` unless we explicitly choose to add them later

## Technical Notes

* Relevant files inspected:
  * [`README.md`](/home/github_projects/etr/README.md)
  * [`crates/etr-control/src/linux.rs`](/home/github_projects/etr/crates/etr-control/src/linux.rs)
  * [`crates/etr-config/src/lib.rs`](/home/github_projects/etr/crates/etr-config/src/lib.rs)
  * [`crates/etrd/src/main.rs`](/home/github_projects/etr/crates/etrd/src/main.rs)
  * [`.trellis/spec/backend/index.md`](/home/github_projects/etr/.trellis/spec/backend/index.md)
  * [`.trellis/spec/guides/index.md`](/home/github_projects/etr/.trellis/spec/guides/index.md)
* Archived requirements reference:
  * [`.trellis/tasks/archive/2026-04/04-12-etr-mvp-routing-service/prd.md`](/home/github_projects/etr/.trellis/tasks/archive/2026-04/04-12-etr-mvp-routing-service/prd.md)
* Current repo search confirms:
  * packaging/install/systemd assets are not yet present in the obvious top-level docs or scripts
  * observability surface described in README is still minimal
  * `etrd` currently uses `clap` with top-level flags only, so adding install-oriented subcommands is feasible without fighting an existing command tree
  * workspace package version is currently defined centrally in [`Cargo.toml`](/home/github_projects/etr/Cargo.toml) as `0.1.0`, which is a natural initial source for artifact versioning
  * there is no existing version bump helper in the repository today

## Research Notes

### Constraints from our repo/project

* Current deployment story is binary + config + optional eBPF object, so a single-binary oriented installation flow fits the current product shape.
* Supporting both an external installer and a runtime `etrd install` path is viable only if both are backed by the same service-definition rendering and filesystem layout rules.
* Auto-detecting `systemd` first and falling back to `/etc/init.d` is operationally convenient, but increases the contract surface for install/uninstall, status handling, and docs.

### Feasible approaches here

**Approach A: self-install canonical path** (Recommended)

* How it works:
  `etrd install` / `etrd uninstall` is the source of truth for service installation. The release bundle may also include a thin shell installer, but that script only places files and invokes the same binary subcommand.
* Pros:
  One canonical install contract, easy binary-only distribution, less drift between "manual install" and "installer install".
* Cons:
  The daemon binary now owns more filesystem/service-manager logic.

**Approach B: external installer canonical path**

* How it works:
  A release script performs file placement and service registration. `etrd install` exists only as a convenience wrapper or not at all.
* Pros:
  Keeps daemon runtime binary simpler.
* Cons:
  More shell logic, higher drift risk, less elegant for direct binary distribution.

**Approach C: dual first-class install paths**

* How it works:
  The external installer and `etrd install` are both full-featured entry points.
* Pros:
  Flexible operator experience.
* Cons:
  Highest maintenance burden unless carefully centralized underneath.

### Release bundle layout options

**Approach A: FHS + versioned bundle** (Selected)

* How it works:
  Ship a versioned tarball such as `etr-vX.Y.Z-linux-x86_64.tar.gz`, with bundle contents arranged for an FHS-style install into `/usr/local`, `/etc`, and `/var/lib`.
* Pros:
  Matches common Linux operator expectations, keeps service definitions straightforward, and makes docs/install behavior clear.
* Cons:
  In-place upgrades need explicit overwrite/backup behavior for binaries and service definitions.

**Approach B: self-contained `/opt` version trees**

* How it works:
  Install versioned payloads under `/opt/etr/<version>` and link a stable executable path.
* Pros:
  Easier side-by-side rollback.
* Cons:
  Heavier layout and more moving pieces for service/runtime paths.

**Approach C: relocatable prefix-first install**

* How it works:
  Center installation around a configurable prefix and make system paths secondary.
* Pros:
  Flexible for custom environments.
* Cons:
  More complex defaults, service generation, and documentation.

### Observability options

**Approach A: JSON debug/status API only** (Selected)

* How it works:
  Add operator-facing JSON endpoints for runtime status, counters, preflight results, and debug-friendly summaries.
* Pros:
  Fastest path to useful diagnostics, easy to consume manually and in tests, avoids premature metrics schema decisions.
* Cons:
  Not directly scrape-ready for standard monitoring stacks.

**Approach B: Prometheus metrics only**

* How it works:
  Expose counters and gauges at `/metrics` but avoid richer JSON debug views.
* Pros:
  Standard monitoring integration.
* Cons:
  Weaker ad hoc troubleshooting story for first operators.

**Approach C: JSON + Prometheus together**

* How it works:
  Ship both operator surfaces in the same milestone.
* Pros:
  Broadest observability coverage.
* Cons:
  Larger scope and more schema maintenance.

### Versioning options

**Approach A: workspace-version date build scheme** (Selected)

* How it works:
  Use the workspace version in [`Cargo.toml`](/home/github_projects/etr/Cargo.toml) as the only version source, formatted as `YYMM.D.BUILD`. A bump tool updates it based on the current date.
* Pros:
  One source of truth for crates and release artifacts, human-readable release chronology, and still compatible with `x.y.z` style version components.
* Cons:
  Requires a small repo-local bump tool and release discipline around when it is invoked.

**Approach B: traditional semver with date in artifact name**

* How it works:
  Keep Cargo version as ordinary semver and put the date only in release metadata or filenames.
* Pros:
  Conventional for Rust packaging.
* Cons:
  Loses the date-first release sequence the user wants.

### Preflight failure handling options

**Approach A: fail closed by default with explicit override** (Selected)

* How it works:
  At bootstrap, validate critical prerequisites such as sysctls, interface readiness, object availability, feature support, and permissions. If required checks fail, exit with clear diagnostics unless the operator explicitly requests degraded bring-up.
* Pros:
  Matches release-quality expectations, prevents false-healthy daemons, and keeps product claims aligned with real runtime readiness.
* Cons:
  Operators doing exploratory setup need an explicit bypass when they want management surfaces before the host is fully prepared.

**Approach B: degraded startup by default**

* How it works:
  Always start the management API and only mark the dataplane as unavailable in status/debug output.
* Pros:
  Easier for exploratory bring-up.
* Cons:
  Higher risk of confusing "process is up but forwarding is broken" states.

## Decision (ADR-lite)

**Context**: The product needs both a convenient binary-only distribution path and an optional installer-driven operator experience, without introducing divergent install/uninstall behaviors.

**Decision**: Make `etrd install` / `etrd uninstall` the canonical installation contract. If a release bundle also ships `install.sh`, that script will remain a thin wrapper that places files and invokes the binary subcommand rather than reimplementing service installation logic.

**Consequences**:

* Service-manager detection, unit/init script rendering, file layout validation, and uninstall behavior must live in Rust rather than shell-only glue.
* Binary-only distribution becomes a first-class operator flow.
* External installer maintenance cost stays low because it delegates to the same core implementation.

**Context**: The release artifact needs a default layout that is operator-friendly, works with service-manager integration, and does not force distro-specific packaging.

**Decision**: Use an FHS-style versioned Linux tarball as the initial release artifact format. The default install layout is `/usr/local/bin/etrd`, `/usr/local/lib/etr/etr-ebpf`, `/etc/etr/etr.toml`, `/var/lib/etr`, and the detected service-manager path.

**Consequences**:

* Install/uninstall logic must handle file creation, overwrite rules, and cleanup in standard system locations.
* Release automation can stay distro-agnostic while still feeling native enough for Linux operators.
* Future `.deb` / `.rpm` packaging remains possible because the filesystem contract is now explicit.

**Context**: The release process needs a single canonical version source that works for Cargo crates and versioned artifacts, while following a date-based `x.y.z`-shaped scheme.

**Decision**: Use [`Cargo.toml`](/home/github_projects/etr/Cargo.toml) workspace version as the only source of truth, with version format `YYMM.D.BUILD`. Implement a bump tool that increments `BUILD` when the stored date matches today's date and resets to today's `YYMM.D.1` when it does not.

**Consequences**:

* All crates continue to inherit the same version automatically.
* Release artifact names can be derived without secondary version config.
* The repo needs a deterministic bump command and documentation for when operators or developers should invoke it.

**Context**: Linux host prerequisites such as `ip_forward`, `rp_filter`, interface state, permissions, and usable eBPF/BTF support are necessary for truthful runtime readiness.

**Decision**: Make startup preflight fail closed by default. If critical checks fail, `etrd` exits with actionable diagnostics. Provide an explicit override for operators who intentionally want degraded bring-up.

**Consequences**:

* Bootstrap must distinguish between hard failures and bypassable warnings.
* Install docs and service templates should surface preflight expectations clearly.
* Status/debug APIs should still report preflight results so operators can see what would have failed and whether an override is in effect.

## Technical Approach

Implement this as a staged Linux productization milestone:

* Release workflow:
  add a repo-local bump tool, build script, and versioned tarball layout derived from workspace version
* Install/uninstall:
  add `etrd` subcommands for canonical install/remove, with shared service rendering and `systemd` to `/etc/init.d` fallback
* Preflight:
  add startup checks and structured diagnostics before dataplane activation, with explicit override support
* Observability:
  add JSON status/debug endpoints exposing preflight state, rule/flow/counter summaries, and relevant runtime diagnostics
* Validation:
  add Linux integration coverage for TCP/UDP forwarding and NAT behavior, then align docs with verified behavior

## Code-Spec Depth Check

This task changes CLI contracts, operator installation behavior, startup validation semantics, and HTTP debug payloads. Implementation should not start without explicit contract depth.

### Target code-spec files to update

* [`.trellis/spec/backend/linux-tc-dataplane.md`](/home/github_projects/etr/.trellis/spec/backend/linux-tc-dataplane.md)
  expand runtime prerequisites and Linux validation expectations to cover productized preflight behavior and validated UDP support
* [`README.md`](/home/github_projects/etr/README.md)
  align published operator workflow, artifact layout, install path, and Linux validation claims with implemented behavior

### Concrete contracts to define and preserve

* CLI signatures:
  * `etrd --config <path> [--bpf-object <path>]`
  * `etrd install [...]`
  * `etrd uninstall [...]`
  * version bump command owned by repo tooling, using workspace `Cargo.toml` as the only version source
* Installation filesystem contract:
  * `/usr/local/bin/etrd`
  * `/usr/local/lib/etr/etr-ebpf`
  * `/etc/etr/etr.toml`
  * `/var/lib/etr`
  * `/etc/systemd/system/etrd.service` or `/etc/init.d/etrd`
* Preflight contract:
  * startup validates critical host prerequisites before dataplane activation
  * critical failures stop startup by default
  * explicit override path is visible in CLI/install flow and reported in debug state
* Debug/status API contract:
  * JSON surfaces expose backend identity, installed rules, preflight results, and runtime diagnostics needed for first-line troubleshooting

### Validation and error matrix

| Condition | Boundary | Expected Behavior |
|----------|----------|-------------------|
| workspace version already matches today's date | bump tool | increment build number only |
| workspace version date differs from today | bump tool | rewrite version to today's `YYMM.D.1` |
| Linux host lacks `ip_forward` or acceptable `rp_filter` | preflight | startup exits with actionable diagnostics unless explicit override is set |
| eBPF object path missing during release install but required for Linux TC mode | install/bootstrap | fail with clear install or startup error |
| `systemd` unavailable but `/etc/init.d` available | install | render and register init.d service instead |
| neither supported service manager path is available | install | fail with actionable diagnostics |
| debug/status endpoint hit after override startup | HTTP/runtime | response clearly shows failed checks and override state |

### Good / Base / Bad cases

Good:

* release tarball `etr-v2604.13.1-linux-x86_64.tar.gz` is produced from workspace version
* `etrd install` installs files to the defined paths and registers a service
* startup preflight passes and debug endpoint reports healthy prerequisite state
* Linux TCP and UDP validation both succeed on the supported host

Base:

* bump tool updates [`Cargo.toml`](/home/github_projects/etr/Cargo.toml) deterministically
* `etrd` still boots in stub mode on non-Linux or Linux without dataplane activation path
* debug endpoint returns structured JSON without requiring packet-capture tools

Bad:

* artifact version and workspace version drift
* install script and `etrd install` produce different filesystem layouts
* service starts successfully but forwarding is impossible because preflight failures were hidden
* README still claims validation is pending after automated/manual validation has been added
