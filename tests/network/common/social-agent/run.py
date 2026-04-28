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
logger.addHandler(logging.StreamHandler(sys.stdout))
logger.setLevel(logging.INFO)

REQUEST_TIMEOUT_SECONDS = 60

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
LOG_VALUE_LIMIT = 160
DEFAULT_CLIENTS = 1


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

    def record_cycle(self, requests_succeeded, requests_total):
        with self.lock:
            self.activity_cycles_total += 1
            self.activity_requests_total += requests_total
            self.activity_requests_success_total += requests_succeeded

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


def local_gateway_base(nodes):
    local_router_host = nodes[NODE_NAME]["router_host"]
    return os.environ.get(
        "ROUTER_GATEWAY_BASE", f"http://{local_router_host}/api/v1/general"
    )


def is_indexed_path(path):
    return bool(path) and isinstance(path, list) and path[0] == -1


def format_log_value(value, limit=LOG_VALUE_LIMIT):
    try:
        text = json.dumps(value, sort_keys=True)
    except TypeError:
        text = repr(value)
    return text if len(text) <= limit else f"{text[: limit - 3]}..."


def get_activity_seconds():
    raw = os.environ.get("ACTIVITY", "")
    if raw == "":
        return 0.0
    return float(raw)


def get_size():
    raw = os.environ.get("SIZE", str(DEFAULT_SIZE))
    size = int(raw)
    if size <= 0:
        logger.warning("SIZE must be positive; defaulting to %s", DEFAULT_SIZE)
        return DEFAULT_SIZE
    return size


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
        "# HELP social_agent_uptime_seconds Uptime of the social agent process.",
        "# TYPE social_agent_uptime_seconds gauge",
        f"social_agent_uptime_seconds {max(time.time() - stats['started'], 0)}",
        "# HELP social_agent_peering_node_info Known node identities for inferred peering graph visualization.",
        "# TYPE social_agent_peering_node_info gauge",
    ]
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


def call(nodes, operation, arguments=None, client_id=None):
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

        url = f"{local_gateway_base(nodes)}/{effective_operation}"

        headers = {"accept": "application/json"}
        if effective_operation not in public_operations:
            headers["x-sync-auth"] = os.environ["SECRET"]

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
    size = get_size()
    activity_seconds = get_activity_seconds()
    clients = get_clients()

    for peer_node in edges.get(NODE_NAME, []):
        peer_router_host = nodes[peer_node]["router_host"]
        while result := call(
            nodes,
            "bridge",
            {
                "name": peer_node,
                "interface": {"*type/string*": f"http://{peer_router_host}/interface"},
            },
            client_id="setup",
        ):
            if result is not True:
                logger.warning(
                    "Could not bridge with %s via %s, trying again",
                    peer_node,
                    peer_router_host,
                )
                time.sleep(1)
            else:
                break

    for i in range(size):
        call(
            nodes,
            "set",
            {
                "path": [["*state*", "data", f"key-{i}"]],
                "value": {"*type/string*": " ".join(choice(WORDS, NUM_WORDS))},
            },
            client_id="setup",
        )

    def _act(client_id):
        successful_requests = 0
        total_requests = 0
        try:
            try:
                path = []
                node_name = NODE_NAME
                traversal = [NODE_NAME]
                while choice(2) and edges.get(node_name):
                    if not edges.get(node_name):
                        break
                    node_name = choice(edges[node_name])
                    traversal.append(node_name)
                    path += [-1, ["*bridge*", node_name, "chain"]]

                path += [-1, ["*state*", "data", f"key-{randint(0, size)}"]]

                if len(traversal) > 1:
                    METRICS.record_inferred_hops(
                        list(zip(traversal[:-1], traversal[1:]))
                    )

                if choice(2):
                    total_requests += 1
                    result = call(nodes, "get", {"path": path}, client_id=client_id)

                    if type(result) is not dict or "*type/string*" not in result:
                        logger.warning("Cannot complete action")
                        return
                    successful_requests += 1

                    words = result["*type/string*"].split(" ")
                    words[randint(0, NUM_WORDS)] = choice(WORDS)

                    total_requests += 1
                    set_result = call(
                        nodes,
                        "set",
                        {
                            "path": [["*state*", "data", f"key-{randint(0, size)}"]],
                            "value": {"*type/string*": " ".join(words)},
                        },
                        client_id=client_id,
                    )
                    if set_result is True:
                        successful_requests += 1
                else:
                    total_requests += 1
                    pin_result = call(nodes, "pin", {"path": path}, client_id=client_id)
                    if pin_result is not True:
                        return
                    successful_requests += 1
                    total_requests += 1
                    unpin_result = call(
                        nodes, "unpin", {"path": path}, client_id=client_id
                    )
                    if unpin_result is True:
                        successful_requests += 1
            except Exception as err:
                logger.warning("Client %s activity cycle failed: %s", client_id, err)
        finally:
            METRICS.record_cycle(successful_requests, total_requests)

    def _client_loop(client_id):
        if activity_seconds <= 0:
            while True:
                _act(client_id)

        until = datetime.now()
        while True:
            _act(client_id)
            time.sleep(max((until - datetime.now()).total_seconds(), 0))
            until += timedelta(seconds=activity_seconds)

    threads = []
    for client_id in range(clients):
        thread = Thread(target=lambda cid=client_id: _client_loop(cid), daemon=True)
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
