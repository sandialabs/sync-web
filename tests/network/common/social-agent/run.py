import json
import logging
import os
import sys
import threading
import time
from datetime import datetime, timedelta
from threading import Thread

import requests
from numpy.random import choice, randint

logger = logging.getLogger(__name__)
if not logger.handlers:
    logger.addHandler(logging.StreamHandler(sys.stdout))
logger.setLevel(logging.INFO)

REQUEST_TIMEOUT_SECONDS = 60

SYNC_USERNAME = os.environ.get("SYNC_USERNAME", "")
SYNC_PASSWORD = os.environ.get("SYNC_PASSWORD", "")
API_TOKEN = ""

DIR = os.path.dirname(os.path.abspath(__file__))

with open(os.path.join(DIR, "frankenstein.txt"), encoding="utf-8-sig") as fd:
    WORDS = "".join(
        x.lower() for x in fd.read() if (x.isascii() and x.isalpha()) or x.isspace()
    ).split()

NUM_WORDS = int(os.environ["WORDS"])
NODE_NAME = os.environ["NODE_NAME"]
PEERS_CONFIG_PATH = os.environ.get("PEERS_CONFIG", os.path.join(DIR, "peers.json"))
METRICS_PATH = os.environ.get(
    "METRICS_TEXTFILE", "/var/lib/node_exporter/textfile/social_agent.prom"
)
BENCHMARK_OUTPUT_PATH = os.environ.get("BENCHMARK_OUTPUT", "")
BENCHMARK_INTERVAL_SECONDS = 1.0
DEFAULT_SIZE = 32
DEFAULT_ACTIVITY = 4.0
DEFAULT_USERS = 1
DEFAULT_SEGMENTS = 2
LOG_VALUE_LIMIT = 160
DEFAULT_CLIENTS = 1
USER_BASE_NAMES = (
    "alice", "bob", "carol", "dave", "eve", "frank", "grace", "heidi",
    "ivan", "judy", "mallory", "michael", "niaj", "olivia", "oscar",
    "peggy", "rupert", "sybil", "trent", "trudy", "victor", "walter",
    "wendy", "xavier", "yvonne", "zara",
)


class Metrics:
    def __init__(self):
        self.lock = threading.Lock()
        self.started = time.time()
        self.requests_total = 0
        self.requests_failed_total = 0
        self.get_latency_sum = 0.0
        self.get_latency_count = 0
        self.set_latency_sum = 0.0
        self.set_latency_count = 0
        self.activity_cycles_total = 0
        self.activity_requests_total = 0
        self.activity_requests_success_total = 0
        self.nodes = set()
        self.inferred_hop_requests_total = {}
        self.user_activity = {}

    def record_request(self, function, duration, success):
        with self.lock:
            self.requests_total += 1
            if not success:
                self.requests_failed_total += 1
            if function == "get":
                self.get_latency_sum += duration
                self.get_latency_count += 1
            elif function in {"set!", "set"}:
                self.set_latency_sum += duration
                self.set_latency_count += 1

    def record_cycle(self, username, requests_succeeded, requests_total):
        with self.lock:
            self.activity_cycles_total += 1
            self.activity_requests_total += requests_total
            self.activity_requests_success_total += requests_succeeded
            user = self.user_activity.setdefault(
                username, {"cycles": 0, "requests": 0, "successes": 0}
            )
            user["cycles"] += 1
            user["requests"] += requests_total
            user["successes"] += requests_succeeded

    def register_nodes(self, nodes):
        with self.lock:
            self.nodes.update(nodes)

    def record_inferred_hops(self, hops):
        with self.lock:
            for src, dst in hops:
                key = (src, dst)
                self.nodes.add(src)
                self.nodes.add(dst)
                self.inferred_hop_requests_total[key] = (
                    self.inferred_hop_requests_total.get(key, 0) + 1
                )

    def snapshot(self):
        with self.lock:
            return {
                "started": self.started,
                "requests_total": self.requests_total,
                "requests_failed_total": self.requests_failed_total,
                "get_latency_sum": self.get_latency_sum,
                "get_latency_count": self.get_latency_count,
                "set_latency_sum": self.set_latency_sum,
                "set_latency_count": self.set_latency_count,
                "activity_cycles_total": self.activity_cycles_total,
                "activity_requests_total": self.activity_requests_total,
                "activity_requests_success_total": self.activity_requests_success_total,
                "nodes": sorted(self.nodes),
                "inferred_hop_requests_total": dict(self.inferred_hop_requests_total),
                "user_activity": {
                    name: dict(values) for name, values in self.user_activity.items()
                },
            }


METRICS = Metrics()


def load_peer_config():
    with open(PEERS_CONFIG_PATH, encoding="utf-8") as fd:
        config = json.load(fd)

    nodes = config.get("nodes", {})
    edges = config.get("edges", {})
    if NODE_NAME not in nodes:
        raise KeyError(f"NODE_NAME {NODE_NAME!r} not present in peers config")
    return nodes, edges


def peer_adjacency(nodes, edges):
    adjacency = {node: set() for node in nodes}
    for source, source_edges in edges.items():
        for edge in source_edges:
            target = edge["node"] if isinstance(edge, dict) else edge
            adjacency[source].add(target)
            adjacency[target].add(source)
    return {node: sorted(peers) for node, peers in adjacency.items()}


def user_names(count):
    """Return deterministic fixture usernames, cycling after the base 26."""
    names = []
    for index in range(count):
        base = USER_BASE_NAMES[index % len(USER_BASE_NAMES)]
        cycle = index // len(USER_BASE_NAMES) + 1
        names.append(base if cycle == 1 else f"{base}-{cycle}")
    return names


def build_user_layout(size):
    """Split each user's fixed keys between public and private directories."""
    public_count = (size + 1) // 2
    return [
        {
            "path": ["data", "public"],
            "keys": [f"key-{index}" for index in range(public_count)],
            "public": True,
        },
        {
            "path": ["data", "private"],
            "keys": [f"key-{index}" for index in range(public_count, size)],
            "public": False,
        },
    ]


def simple_routes(adjacency, start, max_segments):
    """Enumerate deterministic non-revisiting routes outward from start."""
    routes = []

    def walk(current, route, visited):
        if len(route) >= max_segments:
            return
        for peer in adjacency[current]:
            if peer in visited:
                continue
            next_route = [*route, peer]
            routes.append(next_route)
            walk(peer, next_route, {*visited, peer})

    walk(start, [], {start})
    return routes


def route_principal(route, username):
    return [*route, "*state*", username]


def reverse_access_route(target, route):
    return list(reversed([target, *route[:-1]]))


def local_proof_path(route, state_path, origin_index=-1):
    path = [origin_index]
    for peer in route:
        path.extend([peer, -1])
    return [*path, *state_path]


def local_gateway_base(nodes):
    local_router_host = nodes[NODE_NAME]["router_host"]
    return os.environ.get(
        "ROUTER_GATEWAY_BASE", f"http://{local_router_host}/api/v1/general"
    )


def acquire_api_token(nodes, username=SYNC_USERNAME, password=SYNC_PASSWORD):
    local_router_host = nodes[NODE_NAME]["router_host"]
    base = f"http://{local_router_host}"
    delay = 2
    while True:
        try:
            flow_resp = requests.get(
                f"{base}/auth/.ory/self-service/login/api",
                timeout=REQUEST_TIMEOUT_SECONDS,
            )
            flow_resp.raise_for_status()
            flow_id = flow_resp.json()["id"]

            login_resp = requests.post(
                f"{base}/auth/.ory/self-service/login?flow={flow_id}",
                json={"identifier": username, "password": password, "method": "password"},
                timeout=REQUEST_TIMEOUT_SECONDS,
            )
            login_resp.raise_for_status()
            session_token = login_resp.json()["session_token"]

            token_resp = requests.post(
                f"{base}/api/v1/tokens",
                headers={"x-session-token": session_token},
                json={},
                timeout=REQUEST_TIMEOUT_SECONDS,
            )
            token_resp.raise_for_status()
            token = token_resp.json()["token"]
            logger.info("API token acquired for %s on %s", username, NODE_NAME)
            return token
        except requests.HTTPError as e:
            logger.warning("API token acquisition failed (%s), retrying in %ds", e, delay)
        except requests.RequestException as e:
            logger.warning("API token acquisition failed (%s), retrying in %ds", e, delay)
        time.sleep(delay)
        delay = min(delay * 2, 16)


def fixture_password(username):
    """Return the deterministic disposable password for a fixture user."""
    return f"{username}-pass"


def ensure_local_identity(username):
    """Create a disposable local Kratos identity, accepting an existing user."""
    node_index = NODE_NAME.rsplit("-", 1)[-1]
    base = f"http://identity-provider-{node_index}:4434/admin/identities"
    existing = requests.get(
        base,
        params={"credentials_identifier": username},
        timeout=REQUEST_TIMEOUT_SECONDS,
    )
    existing.raise_for_status()
    if existing.json():
        return
    response = requests.post(
        base,
        json={
            "schema_id": "default",
            "traits": {"username": username},
            "credentials": {
                "password": {"config": {"password": fixture_password(username)}}
            },
        },
        timeout=REQUEST_TIMEOUT_SECONDS,
    )
    if response.status_code not in {200, 201, 409}:
        response.raise_for_status()


def is_indexed_path(path):
    return bool(path) and isinstance(path, list) and path[0] == -1


def text_to_byte_vector(text):
    return {"*type/byte-vector*": text.encode("utf-8").hex()}


def byte_vector_text(value):
    if not isinstance(value, dict):
        return None
    hex_text = value.get("*type/byte-vector*")
    if not isinstance(hex_text, str):
        return None
    try:
        return bytes.fromhex(hex_text).decode("utf-8")
    except (ValueError, UnicodeDecodeError):
        return None


def format_log_value(value, limit=LOG_VALUE_LIMIT):
    try:
        text = json.dumps(value, sort_keys=True)
    except TypeError:
        text = repr(value)
    return text if len(text) <= limit else f"{text[: limit - 3]}..."


def get_activity_seconds():
    raw = os.environ.get("ACTIVITY", str(DEFAULT_ACTIVITY))
    if raw == "":
        return DEFAULT_ACTIVITY
    return float(raw)


def get_size():
    raw = os.environ.get("SIZE", str(DEFAULT_SIZE))
    size = int(raw)
    if size < 0:
        logger.warning("SIZE must be non-negative; defaulting to %s", DEFAULT_SIZE)
        return DEFAULT_SIZE
    return size


def get_users():
    users = int(os.environ.get("USERS", str(DEFAULT_USERS)))
    if users < 0:
        logger.warning("USERS must be non-negative; defaulting to %s", DEFAULT_USERS)
        return DEFAULT_USERS
    return users


def get_segments():
    segments = int(os.environ.get("SEGMENTS", str(DEFAULT_SEGMENTS)))
    if segments < 0:
        logger.warning("SEGMENTS must be non-negative; defaulting to %s", DEFAULT_SEGMENTS)
        return DEFAULT_SEGMENTS
    return segments


def get_clients():
    raw = os.environ.get("CLIENTS", str(DEFAULT_CLIENTS))
    clients = int(raw)
    if clients <= 0:
        logger.warning("CLIENTS must be positive; defaulting to %s", DEFAULT_CLIENTS)
        return DEFAULT_CLIENTS
    return clients


def write_metrics():
    stats = METRICS.snapshot()

    def _escape_label(value):
        return value.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")

    lines = [
        "# HELP social_agent_requests_total Total journal interface requests sent by the social agent.",
        "# TYPE social_agent_requests_total counter",
        f"social_agent_requests_total {stats['requests_total']}",
        "# HELP social_agent_requests_failed_total Total journal interface requests that failed.",
        "# TYPE social_agent_requests_failed_total counter",
        f"social_agent_requests_failed_total {stats['requests_failed_total']}",
        "# HELP social_agent_get_latency_seconds_sum Accumulated latency in seconds for get requests.",
        "# TYPE social_agent_get_latency_seconds_sum counter",
        f"social_agent_get_latency_seconds_sum {stats['get_latency_sum']}",
        "# HELP social_agent_get_latency_seconds_count Total get requests observed for latency tracking.",
        "# TYPE social_agent_get_latency_seconds_count counter",
        f"social_agent_get_latency_seconds_count {stats['get_latency_count']}",
        "# HELP social_agent_set_latency_seconds_sum Accumulated latency in seconds for set requests.",
        "# TYPE social_agent_set_latency_seconds_sum counter",
        f"social_agent_set_latency_seconds_sum {stats['set_latency_sum']}",
        "# HELP social_agent_set_latency_seconds_count Total set requests observed for latency tracking.",
        "# TYPE social_agent_set_latency_seconds_count counter",
        f"social_agent_set_latency_seconds_count {stats['set_latency_count']}",
        "# HELP social_agent_activity_cycles_total Total activity cycles attempted.",
        "# TYPE social_agent_activity_cycles_total counter",
        f"social_agent_activity_cycles_total {stats['activity_cycles_total']}",
        "# HELP social_agent_activity_requests_total Total activity requests attempted inside client cycles.",
        "# TYPE social_agent_activity_requests_total counter",
        f"social_agent_activity_requests_total {stats['activity_requests_total']}",
        "# HELP social_agent_activity_requests_success_total Total successful activity requests inside client cycles.",
        "# TYPE social_agent_activity_requests_success_total counter",
        f"social_agent_activity_requests_success_total {stats['activity_requests_success_total']}",
        "# HELP social_agent_user_activity_requests_total Activity requests attempted per fixture user.",
        "# TYPE social_agent_user_activity_requests_total counter",
        "# HELP social_agent_user_activity_requests_success_total Successful activity requests per fixture user.",
        "# TYPE social_agent_user_activity_requests_success_total counter",
        "# HELP social_agent_user_activity_cycles_total Activity cycles attempted per fixture user.",
        "# TYPE social_agent_user_activity_cycles_total counter",
    ]
    for username, values in sorted(stats["user_activity"].items()):
        label = _escape_label(username)
        lines.append(
            f'social_agent_user_activity_requests_total{{user="{label}"}} {values["requests"]}'
        )
        lines.append(
            f'social_agent_user_activity_requests_success_total{{user="{label}"}} {values["successes"]}'
        )
        lines.append(
            f'social_agent_user_activity_cycles_total{{user="{label}"}} {values["cycles"]}'
        )
    lines.extend([
        "# HELP social_agent_uptime_seconds Uptime of the social agent process.",
        "# TYPE social_agent_uptime_seconds gauge",
        f"social_agent_uptime_seconds {max(time.time() - stats['started'], 0)}",
        "# HELP social_agent_peering_node_info Known node identities for inferred peering graph visualization.",
        "# TYPE social_agent_peering_node_info gauge",
    ])
    for node in stats["nodes"]:
        node_label = _escape_label(node)
        lines.append(
            f'social_agent_peering_node_info{{id="{node_label}",title="{node_label}"}} 1'
        )

    lines.extend(
        [
            "# HELP social_agent_inferred_hop_requests_total Count of inferred successful node-to-node hop communications observed via extended reads.",
            "# TYPE social_agent_inferred_hop_requests_total counter",
        ]
    )
    for (src, dst), count in sorted(
        stats["inferred_hop_requests_total"].items(), key=lambda item: item[0]
    ):
        src_label = _escape_label(src)
        dst_label = _escape_label(dst)
        edge_id = _escape_label(f"{src_label}->{dst_label}")
        lines.append(
            'social_agent_inferred_hop_requests_total{id="%s",source="%s",target="%s",secondaryStat="msg/s"} %s'
            % (edge_id, src_label, dst_label, count)
        )

    tmp_path = f"{METRICS_PATH}.tmp"
    os.makedirs(os.path.dirname(METRICS_PATH), exist_ok=True)
    with open(tmp_path, "w", encoding="utf-8") as fd:
        fd.write("\n".join(lines))
        fd.write("\n")
    os.replace(tmp_path, METRICS_PATH)


def make_benchmark_snapshot(stats, now, previous=None):
    uptime_seconds = max(now - stats["started"], 0.0)
    get_latency_avg = (
        stats["get_latency_sum"] / stats["get_latency_count"]
        if stats["get_latency_count"] > 0
        else 0.0
    )
    set_latency_avg = (
        stats["set_latency_sum"] / stats["set_latency_count"]
        if stats["set_latency_count"] > 0
        else 0.0
    )

    snapshot = {
        "node_name": NODE_NAME,
        "timestamp": datetime.utcfromtimestamp(now).isoformat() + "Z",
        "uptime_seconds": uptime_seconds,
        "requests_total": stats["requests_total"],
        "requests_failed_total": stats["requests_failed_total"],
        "requests_succeeded_total": stats["requests_total"]
        - stats["requests_failed_total"],
        "get_requests_total": stats["get_latency_count"],
        "set_requests_total": stats["set_latency_count"],
        "get_latency_sum": stats["get_latency_sum"],
        "get_latency_count": stats["get_latency_count"],
        "set_latency_sum": stats["set_latency_sum"],
        "set_latency_count": stats["set_latency_count"],
        "activity_cycles_total": stats["activity_cycles_total"],
        "activity_requests_total": stats["activity_requests_total"],
        "activity_requests_success_total": stats["activity_requests_success_total"],
        "user_activity": stats.get("user_activity", {}),
        "average_get_latency_seconds": get_latency_avg,
        "average_set_latency_seconds": set_latency_avg,
        "requests_per_second_lifetime": (
            stats["requests_total"] / uptime_seconds if uptime_seconds > 0 else 0.0
        ),
        "activity_cycles_per_second_lifetime": (
            stats["activity_cycles_total"] / uptime_seconds
            if uptime_seconds > 0
            else 0.0
        ),
        "activity_requests_per_second_lifetime": (
            stats["activity_requests_success_total"] / uptime_seconds
            if uptime_seconds > 0
            else 0.0
        ),
    }

    if previous is None:
        snapshot.update(
            {
                "requests_per_second": 0.0,
                "get_requests_per_second": 0.0,
                "set_requests_per_second": 0.0,
                "activity_cycles_per_second": 0.0,
                "activity_requests_per_second": 0.0,
                "activity_request_success_rate": 100.0,
            }
        )
        return snapshot

    elapsed = max(now - previous["timestamp"], 1e-9)
    previous_stats = previous["stats"]
    snapshot.update(
        {
            "requests_per_second": (
                (stats["requests_total"] - previous_stats["requests_total"]) / elapsed
            ),
            "get_requests_per_second": (
                (stats["get_latency_count"] - previous_stats["get_latency_count"])
                / elapsed
            ),
            "set_requests_per_second": (
                (stats["set_latency_count"] - previous_stats["set_latency_count"])
                / elapsed
            ),
            "activity_cycles_per_second": (
                (
                    stats["activity_cycles_total"]
                    - previous_stats["activity_cycles_total"]
                )
                / elapsed
            ),
            "activity_requests_per_second": (
                (
                    stats["activity_requests_success_total"]
                    - previous_stats["activity_requests_success_total"]
                )
                / elapsed
            ),
            "activity_request_success_rate": (
                (
                    stats["activity_requests_success_total"]
                    - previous_stats["activity_requests_success_total"]
                )
                / max(
                    (
                        stats["activity_requests_total"]
                        - previous_stats["activity_requests_total"]
                    ),
                    1e-9,
                )
            )
            * 100.0,
        }
    )
    return snapshot


def write_benchmark_snapshot(previous=None):
    if not BENCHMARK_OUTPUT_PATH:
        return previous

    now = time.time()
    stats = METRICS.snapshot()
    snapshot = make_benchmark_snapshot(stats, now, previous=previous)
    tmp_path = f"{BENCHMARK_OUTPUT_PATH}.tmp"
    os.makedirs(os.path.dirname(BENCHMARK_OUTPUT_PATH), exist_ok=True)
    with open(tmp_path, "w", encoding="utf-8") as fd:
        json.dump(snapshot, fd, indent=2, sort_keys=True)
        fd.write("\n")
    os.replace(tmp_path, BENCHMARK_OUTPUT_PATH)
    return {"timestamp": now, "stats": stats}


def metrics_writer():
    while True:
        try:
            write_metrics()
        except Exception:
            logger.exception("Failed writing metrics")
        time.sleep(1)


def benchmark_writer():
    previous = None
    while True:
        try:
            previous = write_benchmark_snapshot(previous=previous)
        except Exception:
            logger.exception("Failed writing benchmark snapshot")
        time.sleep(BENCHMARK_INTERVAL_SECONDS)


def wait_for_federation_ready(nodes, checks, token):
    """Wait until every fixed route accepts its expected signed staged read."""
    headers = {
        "accept": "application/json",
        "authorization": f"Bearer {token}",
        "content-type": "application/json",
    }
    url = f"{local_gateway_base(nodes)}/get"
    for username, route, path in checks:
        user_token = token[username] if isinstance(token, dict) else token
        headers["authorization"] = f"Bearer {user_token}"
        while True:
            try:
                response = requests.post(
                    url,
                    headers=headers,
                    json={
                        "path": path,
                        "$federation": {"route": route},
                    },
                    timeout=REQUEST_TIMEOUT_SECONDS,
                )
                if response.ok:
                    logger.info("Federation route %s is ready", " -> ".join(route))
                    break
                logger.warning(
                    "Federation route %s is not ready yet (HTTP %s)",
                    " -> ".join(route),
                    response.status_code,
                )
            except requests.RequestException as err:
                logger.warning(
                    "Federation route %s is not ready yet: %s",
                    " -> ".join(route),
                    err,
                )
            time.sleep(1)


def call(nodes, operation, arguments=None, client_id=None, token=None):
    started = time.perf_counter()
    success = False
    try:
        public_operations = {"size", "info", "synchronize", "trace"}
        get_only_operations = {"size", "info"}
        effective_operation = operation
        body = arguments if arguments is not None else {}
        if operation == "get" and isinstance(body, dict):
            path = body.get("path")
            if is_indexed_path(path):
                effective_operation = "resolve"
                body = {
                    "path": path,
                    "pinned?": False,
                    "proof?": False,
                }
                if "expression?" in arguments:
                    body["expression?"] = arguments["expression?"]

        url = f"{local_gateway_base(nodes)}/{effective_operation}"

        headers = {"accept": "application/json"}
        if effective_operation not in public_operations:
            headers["authorization"] = f"Bearer {token or API_TOKEN}"

        if effective_operation in get_only_operations:
            response = requests.get(
                url, headers=headers, timeout=REQUEST_TIMEOUT_SECONDS
            )
        else:
            headers["content-type"] = "application/json"
            response = requests.post(
                url, headers=headers, json=body, timeout=REQUEST_TIMEOUT_SECONDS
            )

        if not response.ok:
            logger.error(
                "%s | %s | %s -> %s",
                f"client {client_id}" if client_id is not None else "client -",
                effective_operation,
                format_log_value(arguments),
                format_log_value(
                    {"status": response.status_code, "body": response.text}
                ),
            )
        response.raise_for_status()
        result = response.json()
        success = True
        logger.info(
            "%s | %s | %s -> %s",
            f"client {client_id}" if client_id is not None else "client -",
            effective_operation,
            format_log_value(arguments),
            format_log_value(result),
        )
        return result
    finally:
        METRICS.record_request(operation, time.perf_counter() - started, success)


def run(nodes, edges):
    global API_TOKEN

    size = get_size()
    activity_seconds = get_activity_seconds()
    clients = get_clients()
    usernames = user_names(get_users())
    segments = get_segments()
    adjacency = peer_adjacency(nodes, edges)
    layout = build_user_layout(size)
    routes_by_target = {
        target: simple_routes(adjacency, target, segments) for target in nodes
    }

    API_TOKEN = acquire_api_token(nodes)
    for username in usernames:
        ensure_local_identity(username)
    user_tokens = {
        username: acquire_api_token(nodes, username, fixture_password(username))
        for username in usernames
    }

    for edge in edges.get(NODE_NAME, []):
        peer_node = edge["node"] if isinstance(edge, dict) else edge
        peer_router_host = nodes[peer_node]["router_host"]
        while True:
            try:
                result = call(
                    nodes,
                    "bridge",
                    {
                        "name": peer_node,
                        "interface": {
                            "*type/string*":
                                f"http://{peer_router_host}/api/v1/journal/interface"
                        },
                        "remote-name": NODE_NAME,
                    },
                    client_id="setup",
                )
            except requests.RequestException as err:
                logger.warning("Bridge with %s is not ready yet: %s", peer_node, err)
                time.sleep(1)
                continue
            if result is True:
                break
            time.sleep(1)

    call(nodes, "set-admins", {"admins": [["*state*", "admin"]]}, client_id="setup")

    for username in usernames:
        owner = ["*state*", username]
        call(
            nodes,
            "authorize",
            {
                "user": owner,
                "rule": {
                    "principal": ["*public*"],
                    "path": ["data", "public"],
                    "get": True,
                    "set!": False,
                    "resolve": True,
                },
            },
            client_id="setup",
        )
        for route in routes_by_target[NODE_NAME]:
            call(
                nodes,
                "authorize",
                {
                    "user": owner,
                    "rule": {
                        "principal": route_principal(route, username),
                        "key-index": [0, -1],
                        "path": ["data", "private"],
                        "get": True,
                        "set!": True,
                        "resolve": True,
                    },
                },
                client_id="setup",
            )

        for bucket in layout:
            for key in bucket["keys"]:
                path = ["*state*", username, *bucket["path"], key]
                existing = call(nodes, "get", {"path": path}, client_id="setup")
                if byte_vector_text(existing) is None:
                    call(
                        nodes,
                        "set",
                        {
                            "path": path,
                            "value": text_to_byte_vector(" ".join(choice(WORDS, NUM_WORDS))),
                        },
                        client_id="setup",
                    )

    assignments = {username: [] for username in usernames}
    readiness_checks = []
    private_bucket = next(bucket for bucket in layout if not bucket["public"])
    public_bucket = next(bucket for bucket in layout if bucket["public"])

    for username in usernames:
        for bucket in layout:
            if bucket["public"] or not bucket["keys"]:
                if bucket["public"]:
                    for key in bucket["keys"]:
                        assignments[username].append({
                            "route": [],
                            "state_path": ["*state*", username, *bucket["path"], key],
                        })
                continue
            for key in bucket["keys"]:
                assignments[username].append({
                    "route": [],
                    "state_path": ["*state*", username, *bucket["path"], key],
                })

        if private_bucket["keys"]:
            for target, terminal_routes in routes_by_target.items():
                for terminal_route in terminal_routes:
                    if terminal_route[-1] != NODE_NAME:
                        continue
                    access_route = reverse_access_route(target, terminal_route)
                    for key in private_bucket["keys"]:
                        assignments[username].append({
                            "route": access_route,
                            "state_path": [
                                "*state*", username, *private_bucket["path"], key
                            ],
                        })
                    readiness_checks.append((
                        username,
                        access_route,
                        ["*state*", username, *private_bucket["path"], private_bucket["keys"][0]],
                    ))
        elif public_bucket["keys"]:
            for peer in adjacency[NODE_NAME]:
                readiness_checks.append((
                    username,
                    [peer],
                    ["*state*", username, *public_bucket["path"], public_bucket["keys"][0]],
                ))

    def act(username, client_id):
        successful_requests = 0
        total_requests = 0
        token = user_tokens[username]
        try:
            try:
                if not assignments[username]:
                    return
                assignment = assignments[username][randint(0, len(assignments[username]))]
                route = assignment["route"]
                state_path = assignment["state_path"]
                federation = {"$federation": {"route": route}} if route else {}
                if route:
                    METRICS.record_inferred_hops(list(zip([NODE_NAME, *route[:-1]], route)))

                if choice(2):
                    total_requests += 1
                    result = call(
                        nodes, "get", {"path": state_path, **federation},
                        client_id=client_id, token=token,
                    )
                    text = byte_vector_text(result)
                    if text is None:
                        return
                    successful_requests += 1
                    words = text.split(" ")
                    if not words:
                        return
                    words[randint(0, len(words))] = choice(WORDS)
                    total_requests += 1
                    if call(
                        nodes,
                        "set",
                        {
                            "path": state_path,
                            "value": text_to_byte_vector(" ".join(words)),
                            **federation,
                        },
                        client_id=client_id,
                        token=token,
                    ) is True:
                        successful_requests += 1
                else:
                    history_path = [-1, *state_path]
                    total_requests += 1
                    if route:
                        total_requests += 1
                        origin_size = call(nodes, "size", client_id=client_id, token=token)
                        if not isinstance(origin_size, int) or origin_size <= 0:
                            return
                        origin_index = origin_size - 1
                        successful_requests += 1
                        total_requests += 1
                        resolved = call(
                            nodes,
                            "resolve",
                            {
                                "path": history_path,
                                "pinned?": False,
                                "proof?": True,
                                "$federation": {
                                    "route": route,
                                    "history": [origin_index, *([-1] * len(route))],
                                },
                            },
                            client_id=client_id,
                            token=token,
                        )
                        proof = resolved.get("proof") if isinstance(resolved, dict) else None
                        if proof is None:
                            return
                        successful_requests += 1
                        pin_args = {
                            "path": local_proof_path(route, state_path, origin_index),
                            "response": proof,
                        }
                    else:
                        pin_args = {"path": history_path}
                    if call(nodes, "pin", pin_args, client_id=client_id, token=token) is not True:
                        return
                    successful_requests += 1
                    total_requests += 1
                    if call(
                        nodes, "unpin", {"path": pin_args["path"]},
                        client_id=client_id, token=token,
                    ) is True:
                        successful_requests += 1
            except Exception as err:
                logger.warning("Client %s activity cycle failed: %s", client_id, err)
        finally:
            METRICS.record_cycle(username, successful_requests, total_requests)

    if activity_seconds <= 0 or not usernames or not any(assignments.values()):
        logger.info("Continuous activity is disabled")
        return

    wait_for_federation_ready(nodes, readiness_checks, user_tokens)

    def client_loop(username, client_index):
        client_id = f"{username}-{client_index}"
        until = datetime.now()
        while True:
            act(username, client_id)
            time.sleep(max((until - datetime.now()).total_seconds(), 0))
            until += timedelta(seconds=activity_seconds)

    threads = []
    for username in usernames:
        for client_index in range(clients):
            thread = Thread(
                target=lambda user=username, index=client_index: client_loop(user, index),
                daemon=True,
            )
            thread.start()
            threads.append(thread)

    for thread in threads:
        thread.join()


if __name__ == "__main__":
    Thread(target=metrics_writer, daemon=True).start()
    if BENCHMARK_OUTPUT_PATH:
        Thread(target=benchmark_writer, daemon=True).start()

    nodes, edges = load_peer_config()

    while True:
        try:
            int(call(nodes, "size"))
            break
        except Exception:
            time.sleep(1)

    METRICS.register_nodes(nodes.keys())
    run(nodes, edges)
