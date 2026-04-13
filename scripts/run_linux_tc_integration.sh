#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=${ETR_TEST_REPO_ROOT:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)}
ETRD_BIN=${ETR_TEST_ETRD_BIN:-"$ROOT_DIR/target/debug/etrd"}
BPF_OBJECT=${ETR_TEST_BPF_OBJECT:-"$ROOT_DIR/target/bpfel-unknown-none/release/etr-ebpf"}

CLIENT_NS="etr-client-$$"
GATEWAY_NS="etr-gateway-$$"
BACKEND_NS="etr-backend-$$"
BRIDGE="etr-br-$$"
TMP_DIR=$(mktemp -d /tmp/etr-linux-it.XXXXXX)
CLIENT_HOST_VETH="etr-cl-host-$$"
CLIENT_NS_VETH="eth0"
GATEWAY_HOST_VETH="etr-gw-host-$$"
GATEWAY_NS_VETH="eth0"
BACKEND_HOST_VETH="etr-be-host-$$"
BACKEND_NS_VETH="eth0"
MGMT_URL="http://127.0.0.1:9911"
TCP_FRONTEND_PORT=18080
UDP_FRONTEND_PORT=16020
TCP_BACKEND_PORT=8081
UDP_BACKEND_PORT=11426
CLIENT_IP="10.10.0.2"
GATEWAY_IP="10.10.0.1"
BACKEND_IP="10.10.0.3"
ETRD_PID=""
BACKEND_PID=""
NETNS_READY=0

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 1
  }
}

require_cap_net_admin() {
  python3 - <<'PY'
import sys

for line in open("/proc/self/status", "r", encoding="utf-8"):
    if line.startswith("CapEff:"):
        value = int(line.split()[1], 16)
        if value & (1 << 12):
            raise SystemExit(0)
        break

raise SystemExit(1)
PY
}

require_namespace_access() {
  local probe_ns="etr-probe-$$"
  if ! ip netns add "$probe_ns" >/dev/null 2>&1; then
    echo "this harness requires permission to create network namespaces in the current environment" >&2
    return 1
  fi
  ip netns del "$probe_ns" >/dev/null 2>&1 || true
}

cleanup() {
  set +e
  if [[ -n "$ETRD_PID" ]]; then
    kill "$ETRD_PID" >/dev/null 2>&1 || true
  fi
  if [[ -n "$BACKEND_PID" ]]; then
    kill "$BACKEND_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$NETNS_READY" == "1" ]]; then
    for ns in "$CLIENT_NS" "$GATEWAY_NS" "$BACKEND_NS"; do
      if ip netns list 2>/dev/null | grep -q "^${ns}\b"; then
        ip netns pids "$ns" | xargs -r kill >/dev/null 2>&1 || true
        ip netns del "$ns" >/dev/null 2>&1 || true
      fi
    done
    ip link del "$BRIDGE" >/dev/null 2>&1 || true
  fi
  rm -rf "$TMP_DIR"
}

on_exit() {
  status=$?
  if (( status != 0 )); then
    echo "integration harness failed with status $status" >&2
    if [[ -f "$TMP_DIR/etrd.log" ]]; then
      echo "=== etrd.log ===" >&2
      cat "$TMP_DIR/etrd.log" >&2
    fi
    if [[ -f "$TMP_DIR/backend.log" ]]; then
      echo "=== backend.log ===" >&2
      cat "$TMP_DIR/backend.log" >&2
    fi
    if [[ -f "$TMP_DIR/debug-after.json" ]]; then
      echo "=== debug-after.json ===" >&2
      cat "$TMP_DIR/debug-after.json" >&2
    fi
  fi
  cleanup
  exit "$status"
}

trap on_exit EXIT

build_artifacts_if_needed() {
  if [[ ! -x "$ETRD_BIN" ]]; then
    cargo build -p etrd
  fi

  if [[ ! -f "$BPF_OBJECT" ]]; then
    RUSTFLAGS="-C debuginfo=2 -C link-arg=--btf" \
    CARGO_TARGET_BPFEL_UNKNOWN_NONE_LINKER=bpf-linker \
    cargo +nightly build -p etr-ebpf \
      --target bpfel-unknown-none \
      -Z build-std=core \
      --release
  fi
}

create_backend_server() {
  cat >"$TMP_DIR/backend_server.py" <<'PY'
import http.server
import socket
import threading

TCP_BIND = ("10.10.0.3", 8081)
UDP_BIND = ("10.10.0.3", 11426)

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"tcp-ok"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt, *args):
        return

def run_udp():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(UDP_BIND)
    while True:
        data, addr = sock.recvfrom(4096)
        sock.sendto(b"udp-ok:" + data, addr)

threading.Thread(target=run_udp, daemon=True).start()
http.server.ThreadingHTTPServer(TCP_BIND, Handler).serve_forever()
PY
}

create_gateway_config() {
  cat >"$TMP_DIR/etr.toml" <<EOF
[service]
node_name = "linux-it-gateway"

[management]
listen = "127.0.0.1:9911"

[data_plane]
kind = "tc"
external_interface = "$GATEWAY_NS_VETH"

[[rules]]
name = "tcp-proxy"
protocol = "tcp"
family = "ipv4"
listen_addr = "0.0.0.0"
listen_port = $TCP_FRONTEND_PORT
snat = "masquerade"

[[rules.backends]]
addr = "$BACKEND_IP"
port = $TCP_BACKEND_PORT

[[rules]]
name = "udp-proxy"
protocol = "udp"
family = "ipv4"
listen_addr = "0.0.0.0"
listen_port = $UDP_FRONTEND_PORT
snat = "masquerade"

[[rules.backends]]
addr = "$BACKEND_IP"
port = $UDP_BACKEND_PORT
EOF
}

create_namespaces() {
  ip netns add "$CLIENT_NS"
  ip netns add "$GATEWAY_NS"
  ip netns add "$BACKEND_NS"

  ip link add "$BRIDGE" type bridge
  ip link set "$BRIDGE" up

  ip link add "$CLIENT_HOST_VETH" type veth peer name "$CLIENT_NS_VETH"
  ip link add "$GATEWAY_HOST_VETH" type veth peer name "$GATEWAY_NS_VETH"
  ip link add "$BACKEND_HOST_VETH" type veth peer name "$BACKEND_NS_VETH"

  ip link set "$CLIENT_NS_VETH" netns "$CLIENT_NS"
  ip link set "$GATEWAY_NS_VETH" netns "$GATEWAY_NS"
  ip link set "$BACKEND_NS_VETH" netns "$BACKEND_NS"

  ip link set "$CLIENT_HOST_VETH" master "$BRIDGE"
  ip link set "$GATEWAY_HOST_VETH" master "$BRIDGE"
  ip link set "$BACKEND_HOST_VETH" master "$BRIDGE"
  ip link set "$CLIENT_HOST_VETH" up
  ip link set "$GATEWAY_HOST_VETH" up
  ip link set "$BACKEND_HOST_VETH" up

  ip -n "$CLIENT_NS" link set lo up
  ip -n "$CLIENT_NS" link set "$CLIENT_NS_VETH" up
  ip -n "$CLIENT_NS" addr add "${CLIENT_IP}/24" dev "$CLIENT_NS_VETH"

  ip -n "$GATEWAY_NS" link set lo up
  ip -n "$GATEWAY_NS" link set "$GATEWAY_NS_VETH" up
  ip -n "$GATEWAY_NS" addr add "${GATEWAY_IP}/24" dev "$GATEWAY_NS_VETH"

  ip -n "$BACKEND_NS" link set lo up
  ip -n "$BACKEND_NS" link set "$BACKEND_NS_VETH" up
  ip -n "$BACKEND_NS" addr add "${BACKEND_IP}/24" dev "$BACKEND_NS_VETH"

  ip netns exec "$GATEWAY_NS" sysctl -w net.ipv4.ip_forward=1 >/dev/null
  ip netns exec "$GATEWAY_NS" sysctl -w net.ipv4.conf."$GATEWAY_NS_VETH".rp_filter=0 >/dev/null
}

wait_for_management() {
  for _ in $(seq 1 30); do
    if ip netns exec "$GATEWAY_NS" python3 - <<'PY' >/dev/null 2>&1
import urllib.request
urllib.request.urlopen("http://127.0.0.1:9911/healthz", timeout=1).read()
PY
    then
      return 0
    fi
    sleep 1
  done

  echo "management API did not become ready" >&2
  return 1
}

fetch_debug_json() {
  ip netns exec "$GATEWAY_NS" python3 - <<'PY'
import urllib.request
print(urllib.request.urlopen("http://127.0.0.1:9911/api/v1/debug/dataplane", timeout=2).read().decode())
PY
}

assert_tc_attached() {
  ip netns exec "$GATEWAY_NS" tc filter show dev "$GATEWAY_NS_VETH" ingress | grep -q "etr_ingress"
  ip netns exec "$GATEWAY_NS" tc filter show dev "$GATEWAY_NS_VETH" egress | grep -q "etr_egress"
}

run_tcp_probe() {
  ip netns exec "$CLIENT_NS" python3 - <<PY
import socket

request = (
    b"GET / HTTP/1.1\r\n"
    b"Host: ${GATEWAY_IP}:${TCP_FRONTEND_PORT}\r\n"
    b"Connection: close\r\n\r\n"
)

with socket.create_connection(("${GATEWAY_IP}", ${TCP_FRONTEND_PORT}), timeout=5) as sock:
    sock.sendall(request)
    data = b""
    while True:
        chunk = sock.recv(4096)
        if not chunk:
            break
        data += chunk

if b"tcp-ok" not in data:
    raise SystemExit("tcp probe did not receive expected backend response")
PY
}

run_udp_probe() {
  ip netns exec "$CLIENT_NS" python3 - <<PY
import socket

message = b"udp-probe"
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.settimeout(5)
sock.sendto(message, ("${GATEWAY_IP}", ${UDP_FRONTEND_PORT}))
data, _ = sock.recvfrom(4096)
sock.close()

if data != b"udp-ok:" + message:
    raise SystemExit(f"unexpected udp response: {data!r}")
PY
}

assert_counter_growth() {
  python3 - "$TMP_DIR/debug-before.json" "$TMP_DIR/debug-after.json" <<'PY'
import json
import sys

before = json.load(open(sys.argv[1], "r", encoding="utf-8"))
after = json.load(open(sys.argv[2], "r", encoding="utf-8"))

before_stats = before["stats"]
after_stats = after["stats"]

checks = {
    "ingress_rule_hits": after_stats["ingress_rule_hits"] > before_stats["ingress_rule_hits"],
    "flow_creations": after_stats["flow_creations"] > before_stats["flow_creations"],
    "ingress_reverse_hits": after_stats["ingress_reverse_hits"] > before_stats["ingress_reverse_hits"],
    "egress_flow_hits": after_stats["egress_flow_hits"] > before_stats["egress_flow_hits"],
}

failed = [name for name, ok in checks.items() if not ok]
if failed:
    raise SystemExit(f"expected dataplane counters to grow, but these did not: {', '.join(failed)}")

if after["preflight"]["passed"] is not True:
    raise SystemExit("preflight was not passing during integration test")
PY
}

main() {
  require_command ip
  require_command tc
  require_command bpftool
  require_command python3
  require_command cargo

  if [[ "$(id -u)" != "0" ]]; then
    echo "this harness requires root" >&2
    exit 1
  fi
  if ! require_cap_net_admin; then
    echo "this harness requires CAP_NET_ADMIN for network namespaces and tc attach" >&2
    exit 1
  fi
  if ! require_namespace_access; then
    exit 1
  fi
  NETNS_READY=1

  build_artifacts_if_needed
  create_backend_server
  create_gateway_config
  create_namespaces

  ip netns exec "$BACKEND_NS" python3 "$TMP_DIR/backend_server.py" >"$TMP_DIR/backend.log" 2>&1 &
  BACKEND_PID=$!

  ip netns exec "$GATEWAY_NS" "$ETRD_BIN" \
    --config "$TMP_DIR/etr.toml" \
    --bpf-object "$BPF_OBJECT" \
    >"$TMP_DIR/etrd.log" 2>&1 &
  ETRD_PID=$!

  wait_for_management
  assert_tc_attached
  fetch_debug_json >"$TMP_DIR/debug-before.json"
  run_tcp_probe
  run_udp_probe
  fetch_debug_json >"$TMP_DIR/debug-after.json"
  assert_counter_growth

  echo "linux tc integration test passed"
}

main "$@"
