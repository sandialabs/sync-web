#!/usr/bin/env python3
"""Qualify sticky federation readiness against a process-isolated network stack.

The stack is expected to come from tests/network/compose. This probe deliberately
uses real Router/Gateway/Journal process boundaries and the social fixture's
users, grants, staged data, and reciprocal bridges.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import re
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import requests


@dataclass(frozen=True)
class Route:
    origin: str
    hops: tuple[str, ...]


def adjacency(peers: dict[str, Any]) -> dict[str, list[str]]:
    result = {name: set() for name in peers["nodes"]}
    for source, edges in peers["edges"].items():
        for edge in edges:
            target = edge["node"] if isinstance(edge, dict) else edge
            result[source].add(target)
            result[target].add(source)
    return {name: sorted(neighbors) for name, neighbors in result.items()}


def simple_routes(graph: dict[str, list[str]], start: str, max_segments: int) -> list[list[str]]:
    result: list[list[str]] = []

    def walk(current: str, route: list[str], seen: set[str]) -> None:
        if len(route) >= max_segments:
            return
        for peer in graph[current]:
            if peer in seen:
                continue
            next_route = [*route, peer]
            result.append(next_route)
            walk(peer, next_route, seen | {peer})

    walk(start, [], {start})
    return result


def readiness_routes(peers: dict[str, Any], max_segments: int) -> list[Route]:
    graph = adjacency(peers)
    routes: list[Route] = []
    for origin in sorted(graph):
        origin_routes: set[tuple[str, ...]] = set()
        for target in sorted(graph):
            for terminal_route in simple_routes(graph, target, max_segments):
                if terminal_route[-1] == origin:
                    origin_routes.add(tuple(reversed([target, *terminal_route[:-1]])))
        routes.extend(Route(origin, route) for route in sorted(origin_routes))
    return routes


def metric_failures(text: str) -> float:
    total = 0.0
    for line in text.splitlines():
        if not line.startswith("sync_gateway_journal_requests_total"):
            continue
        key, raw_value = line.rsplit(" ", 1)
        if 'result="success"' not in key:
            total += float(raw_value)
    return total


def proof_path(route: Route, key_path: list[Any]) -> list[Any]:
    result: list[Any] = [-1]
    for hop in route.hops:
        result.extend((hop, -1))
    result.extend(key_path)
    return result


def parse_serializer_oracle(raw: str) -> dict[str, Any]:
    kinds = {}
    for label in ("before", "plain", "traced"):
        match = re.search(rf"\({label} ([^)]+)\)", raw)
        if not match:
            raise ValueError(f"serializer oracle is missing {label}")
        kinds[label] = match.group(1)
    digests = {}
    for label in ("digest", "plain-digest", "traced-digest"):
        match = re.search(rf"\({label} #u\(([^)]+)\)\)", raw)
        if not match:
            raise ValueError(f"serializer oracle is missing {label}")
        values = bytes(int(value) for value in match.group(1).split())
        digests[label] = values.hex()
    return {"kinds": kinds, "digests": digests}


def structural_proof_check(body: Any) -> dict[str, Any]:
    content = body.get("content") if isinstance(body, dict) else None
    proof = body.get("proof") if isinstance(body, dict) else None
    typed_content = isinstance(content, dict) and isinstance(content.get("*type/byte-vector*"), str)
    pinned_false = isinstance(body, dict) and body.get("pinned?") is False
    root_present = isinstance(proof, dict) and "n-1" in proof
    missing: set[str] = set()

    def visit(value: Any) -> None:
        if isinstance(value, str) and re.fullmatch(r"n-\d+", value):
            if value != "n-0" and (not isinstance(proof, dict) or value not in proof):
                missing.add(value)
        elif isinstance(value, list):
            for item in value:
                visit(item)
        elif isinstance(value, dict):
            for item in value.values():
                visit(item)

    if isinstance(proof, dict):
        visit(proof)
    canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return {
        "ok": typed_content and pinned_false and root_present and not missing,
        "typed_content": typed_content,
        "pinned_false": pinned_false,
        "root_present": root_present,
        "proof_closed": isinstance(proof, dict) and not missing,
        "missing_refs": sorted(missing),
        "proof_nodes": len(proof) if isinstance(proof, dict) else None,
        "proof_sha256": hashlib.sha256(canonical(proof)).hexdigest() if isinstance(proof, dict) else None,
        "root_node_sha256": hashlib.sha256(canonical(proof["n-1"])).hexdigest() if root_present else None,
    }


def readiness_transition(ready: bool, failed: bool) -> str:
    if failed:
        return "terminal" if ready else "retry"
    return "continue" if ready else "ready"


def host_state() -> dict[str, Any]:
    memory = {
        key: int(value.strip().split()[0])
        for key, value in (line.split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines())
    }
    vmstat = {
        key: int(value)
        for key, value in (line.split() for line in Path("/proc/vmstat").read_text().splitlines())
        if key == "oom_kill"
    }
    full = next(line for line in Path("/proc/pressure/memory").read_text().splitlines() if line.startswith("full "))
    pressure = {key: float(value) for key, value in (field.split("=") for field in full.split()[1:3])}
    return {
        "mem_available_kib": memory["MemAvailable"],
        "oom_kill": vmstat["oom_kill"],
        "memory_full_avg10": pressure["avg10"],
    }


class Qualifier:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.output = Path(args.output)
        self.output.mkdir(parents=True, exist_ok=True)
        self.results = Path(args.results)
        self.peers = json.loads(Path(args.peers).read_text(encoding="utf-8"))
        self.routes = readiness_routes(self.peers, args.max_segments)
        self.nodes = sorted(self.peers["nodes"])
        self.router = {
            node: f"http://127.0.0.1:{args.router_port_base + int(node.rsplit('-', 1)[1])}"
            for node in self.nodes
        }
        self.gateway = {
            node: f"http://127.0.0.1:{args.gateway_port_base + int(node.rsplit('-', 1)[1])}"
            for node in self.nodes
        }
        self.tokens: dict[str, str] = {}
        self.deadline = 0.0
        self.initial_host = host_state()

    def write_json(self, name: str, value: Any) -> None:
        path = self.output / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def append_json(self, name: str, value: Any) -> None:
        with (self.output / name).open("a", encoding="utf-8") as output:
            output.write(json.dumps(value, sort_keys=True) + "\n")

    def request(self, method: str, url: str, **kwargs: Any) -> requests.Response:
        remaining = max(0.2, self.deadline - time.monotonic()) if self.deadline else self.args.request_timeout
        return requests.request(method, url, timeout=min(self.args.request_timeout, remaining), **kwargs)

    def token(self, node: str) -> str:
        base = self.router[node]
        flow = self.request("GET", f"{base}/auth/.ory/self-service/login/api").json()["id"]
        session = self.request(
            "POST",
            f"{base}/auth/.ory/self-service/login?flow={flow}",
            json={"identifier": self.args.username, "password": self.args.password, "method": "password"},
        )
        session.raise_for_status()
        created = self.request(
            "POST",
            f"{base}/api/v1/tokens",
            headers={"x-session-token": session.json()["session_token"]},
            json={"description": "sticky readiness QA"},
        )
        created.raise_for_status()
        return created.json()["token"]

    def agent_snapshot(self, index: int) -> dict[str, Any]:
        return json.loads((self.results / f"social-agent-{index}" / "benchmark.json").read_text(encoding="utf-8"))

    def snapshot(self) -> dict[str, Any]:
        agents: list[dict[str, Any]] = [{} for _ in self.nodes]
        gateway_failures: list[float] = [0.0 for _ in self.nodes]

        def agent(index: int) -> tuple[str, int, Any]:
            return "agent", index, self.agent_snapshot(index)

        def gateway(index: int) -> tuple[str, int, Any]:
            response = self.request("GET", f"{self.gateway[self.nodes[index]]}/metrics")
            response.raise_for_status()
            return "gateway", index, metric_failures(response.text)

        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as executor:
            futures = [executor.submit(agent, index) for index in range(len(self.nodes))]
            futures.extend(executor.submit(gateway, index) for index in range(len(self.nodes)))
            for future in concurrent.futures.as_completed(futures):
                kind, index, value = future.result()
                if kind == "agent":
                    agents[index] = value
                else:
                    gateway_failures[index] = value
        host = host_state()
        if (
            host["mem_available_kib"] < self.args.minimum_mem_gib * 1024 * 1024
            or host["oom_kill"] > self.initial_host["oom_kill"]
            or host["memory_full_avg10"] > self.args.maximum_memory_full_psi
        ):
            raise RuntimeError(f"host resource gate failed: {host}")
        return {
            "monotonic_ns": time.monotonic_ns(),
            "cycles": [int(value.get("activity_cycles_total", 0)) for value in agents],
            "agent_failures": [float(value.get("requests_failed_total", 0)) for value in agents],
            "gateway_failures": gateway_failures,
            "host": host,
        }

    def wait_for_workers(self) -> None:
        deadline = time.monotonic() + self.args.startup_timeout
        while time.monotonic() < deadline:
            try:
                if all(self.agent_snapshot(index).get("activity_cycles_total", 0) > 0 for index in range(4)):
                    return
            except (FileNotFoundError, json.JSONDecodeError):
                pass
            time.sleep(1)
        raise RuntimeError("social-agent workers did not begin cycling")

    def probe(self, route: Route) -> dict[str, Any]:
        headers = {
            "authorization": f"Bearer {self.tokens[route.origin]}",
            "content-type": "application/json",
            "accept": "application/json",
        }
        key_path = ["*state*", self.args.username, "data", "private", self.args.key]
        get_response = self.request(
            "POST",
            f"{self.router[route.origin]}/api/v1/general/get",
            headers=headers,
            json={"path": key_path, "$federation": {"route": list(route.hops)}},
        )
        resolve_response = self.request(
            "POST",
            f"{self.router[route.origin]}/api/v1/general/resolve",
            headers=headers,
            json={"path": proof_path(route, key_path), "pinned?": False, "proof?": True},
        )
        try:
            get_body = get_response.json()
        except ValueError:
            get_body = None
        try:
            resolve_body = resolve_response.json()
        except ValueError:
            resolve_body = None
        get_ok = get_response.ok and isinstance(get_body, dict) and isinstance(get_body.get("*type/byte-vector*"), str)
        proof_check = structural_proof_check(resolve_body)
        resolve_ok = resolve_response.ok and proof_check["ok"]
        return {
            "origin": route.origin,
            "route": list(route.hops),
            "get_status": get_response.status_code,
            "get_error": None if get_ok else get_body,
            "resolve_status": resolve_response.status_code,
            "resolve_error": None if resolve_ok else resolve_body,
            "proof": proof_check,
            "ok": get_ok and resolve_ok,
        }

    def run(self) -> int:
        self.write_json(
            "routes.json",
            {
                "peers": self.peers,
                "route_count": len(self.routes),
                "routes": [{"origin": route.origin, "route": list(route.hops)} for route in self.routes],
            },
        )
        self.wait_for_workers()
        self.tokens = {node: self.token(node) for node in self.nodes}
        baseline = self.snapshot()
        dwell_started = time.monotonic()
        self.deadline = time.monotonic() + self.args.qualification_timeout
        streak = {route: 0 for route in self.routes}
        ready = False
        pre_ready_failures = 0
        sweeps = 0

        while time.monotonic() < self.deadline:
            sweeps += 1
            with concurrent.futures.ThreadPoolExecutor(max_workers=self.args.probe_workers) as executor:
                results = list(executor.map(self.probe, self.routes))
            snapshot = self.snapshot()
            route_failed = any(not result["ok"] for result in results)
            counter_failed = (
                sum(snapshot["agent_failures"]) > sum(baseline["agent_failures"])
                or sum(snapshot["gateway_failures"]) > sum(baseline["gateway_failures"])
            )
            self.append_json("sweeps.jsonl", {"sweep": sweeps, "results": results, "snapshot": snapshot})
            transition = readiness_transition(ready, route_failed or counter_failed)
            if transition == "retry":
                pre_ready_failures += 1
                baseline = snapshot
                dwell_started = time.monotonic()
                time.sleep(self.args.sweep_delay)
                continue
            if transition == "terminal":
                outcome = {
                    "pass": False,
                    "reason": "post-ready sticky federation failure",
                    "sweeps": sweeps,
                    "resets": 1,
                    "pre_ready_failures": pre_ready_failures,
                    "stable_seconds": time.monotonic() - dwell_started,
                    "route_failed": route_failed,
                    "counter_failed": counter_failed,
                    "failed_routes": [result for result in results if not result["ok"]],
                    "baseline": baseline,
                    "end": snapshot,
                }
                self.write_json("outcome.json", outcome)
                return 1
            if transition == "ready":
                ready = True
                baseline = snapshot
                dwell_started = time.monotonic()
                streak = {route: 1 for route in self.routes}
            else:
                for route in self.routes:
                    streak[route] += 1
            enough_time = time.monotonic() - dwell_started >= self.args.stable_seconds
            enough_cycles = all(
                snapshot["cycles"][index] - baseline["cycles"][index] >= self.args.minimum_cycles
                for index in range(len(self.nodes))
            )
            enough_routes = all(value >= self.args.minimum_sweeps for value in streak.values())
            if enough_time and enough_cycles and enough_routes:
                outcome = {
                    "pass": True,
                    "sweeps": sweeps,
                    "resets": 0,
                    "pre_ready_failures": pre_ready_failures,
                    "stable_seconds": time.monotonic() - dwell_started,
                    "cycle_deltas": [
                        snapshot["cycles"][index] - baseline["cycles"][index]
                        for index in range(len(self.nodes))
                    ],
                    "route_streak": min(streak.values()),
                    "baseline": baseline,
                    "end": snapshot,
                }
                self.write_json("outcome.json", outcome)
                return 0
            time.sleep(self.args.sweep_delay)

        outcome = {
            "pass": False,
            "reason": "no stable process-isolated federation dwell",
            "sweeps": sweeps,
            "resets": 0,
            "pre_ready_failures": pre_ready_failures,
            "ready": ready,
            "stable_seconds": time.monotonic() - dwell_started,
            "baseline": baseline,
            "end": self.snapshot(),
        }
        self.write_json("outcome.json", outcome)
        return 1


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--peers", required=True)
    parser.add_argument("--results", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--router-port-base", type=int, default=8192)
    parser.add_argument("--gateway-port-base", type=int, default=8292)
    parser.add_argument("--username", default="alice")
    parser.add_argument("--password", default="alice-pass")
    parser.add_argument("--key", default="key-4")
    parser.add_argument("--max-segments", type=int, default=2)
    parser.add_argument("--stable-seconds", type=float, default=30)
    parser.add_argument("--minimum-cycles", type=int, default=10)
    parser.add_argument("--minimum-sweeps", type=int, default=3)
    parser.add_argument("--qualification-timeout", type=float, default=300)
    parser.add_argument("--startup-timeout", type=float, default=300)
    parser.add_argument("--request-timeout", type=float, default=30)
    parser.add_argument("--probe-workers", type=int, default=8)
    parser.add_argument("--sweep-delay", type=float, default=1)
    parser.add_argument("--minimum-mem-gib", type=float, default=8)
    parser.add_argument("--maximum-memory-full-psi", type=float, default=1)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    qualifier = Qualifier(args)
    try:
        return qualifier.run()
    except Exception as error:
        qualifier.write_json("outcome.json", {"pass": False, "reason": str(error)})
        raise


if __name__ == "__main__":
    raise SystemExit(main())
