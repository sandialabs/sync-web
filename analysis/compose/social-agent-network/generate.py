#!/usr/bin/env python3

import copy
import json
import os
import random
from pathlib import Path

try:
    import yaml
except ModuleNotFoundError as exc:
    raise SystemExit("PyYAML is required to run this generator") from exc


SCRIPT_DIR = Path(__file__).resolve().parent
OUTPUT_COMPOSE = SCRIPT_DIR / "docker-compose.yml"
OUTPUT_PEERS = SCRIPT_DIR / "peers.json"
HTTP_ONLY_CERT = SCRIPT_DIR / "http-only.crt"
HTTP_ONLY_KEY = SCRIPT_DIR / "http-only.key"
ACME_DIR = SCRIPT_DIR / "acme-challenge"
METRICS_DIR = SCRIPT_DIR / "metrics"
RESULTS_DIR = SCRIPT_DIR / "results"
SOCIAL_AGENT_VERSION = (
    SCRIPT_DIR.parents[1]
    / "firewheel"
    / "model-components"
    / "social-agent"
    / "version.txt"
).read_text(encoding="utf-8").strip()

HTTP_PORT_BASE = 8192
SMB_PORT_BASE = 1445
FILE_SYSTEM_HOST_PORT_ZERO = 445
DEFAULT_NODE_COUNT = 4
DEFAULT_SECRET = "pass"
DEFAULT_CONNECTIVITY = 2
DEFAULT_PERIOD = 2
DEFAULT_WINDOW = 1024
DEFAULT_SIZE = 32
DEFAULT_ACTIVITY = 0.0
DEFAULT_WORDS = 8
DEFAULT_CLIENTS = 1
AGGREGATE_RESULTS_PORT = 8290


def env_required(name):
    value = os.environ.get(name)
    if value is None or value == "":
        raise SystemExit(f"Environment variable {name} is required")
    return value


def env_optional(name, default):
    value = os.environ.get(name)
    if value is None or value == "":
        return default
    return value


def env_int(name, default):
    value = env_optional(name, default)
    try:
        return int(value)
    except ValueError as exc:
        raise SystemExit(f"Environment variable {name} must be an integer") from exc


def env_float(name, default):
    value = env_optional(name, default)
    try:
        return float(value)
    except ValueError as exc:
        raise SystemExit(f"Environment variable {name} must be numeric") from exc


def load_base_compose(path):
    with path.open(encoding="utf-8") as fd:
        data = yaml.safe_load(fd)
    if not isinstance(data, dict):
        raise SystemExit("Base compose file must parse to a mapping")
    if "services" not in data or not isinstance(data["services"], dict):
        raise SystemExit("Base compose file must contain a services mapping")
    return data


def to_env_map(environment):
    if environment is None:
        return {}
    if isinstance(environment, dict):
        return dict(environment)
    if isinstance(environment, list):
        result = {}
        for item in environment:
            if "=" in item:
                key, value = item.split("=", 1)
                result[key] = value
        return result
    raise SystemExit("Unsupported compose environment format")


def rewrite_depends_on(depends_on, node_index):
    if depends_on is None:
        return None
    if isinstance(depends_on, list):
        return [f"{service}-{node_index}" for service in depends_on]
    if isinstance(depends_on, dict):
        return {
            f"{service}-{node_index}": copy.deepcopy(config)
            for service, config in depends_on.items()
        }
    raise SystemExit("Unsupported compose depends_on format")


def rewrite_service_environment(service_name, node_index, environment, secret, period, window):
    env_map = to_env_map(environment)

    if service_name == "journal":
        env_map["SECRET"] = secret
        env_map["PERIOD"] = str(period)
        env_map["WINDOW"] = str(window)
    elif service_name == "gateway":
        env_map["JOURNAL_JSON_ENDPOINT"] = f"http://journal-{node_index}/interface/json"
        env_map["JOURNAL_LISP_ENDPOINT"] = f"http://journal-{node_index}/interface"
    elif service_name == "router":
        env_map["ROUTER_JOURNAL_HOST"] = f"journal-{node_index}"
        env_map["ROUTER_GATEWAY_HOST"] = f"gateway-{node_index}"
        env_map["ROUTER_EXPLORER_HOST"] = f"explorer-{node_index}"
        env_map["ROUTER_WORKBENCH_HOST"] = f"workbench-{node_index}"
    elif service_name == "file-system":
        env_map["SYNC_FS_GatewayBaseUrl"] = f"http://gateway-{node_index}/api/v1"
        env_map["SYNC_FS_GatewayAuthToken"] = secret
        env_map["SYNC_FS_JournalJsonUrl"] = f"http://journal-{node_index}/interface/json"

    return env_map


def rewrite_volume_entry(entry, node_index, named_volumes):
    if not isinstance(entry, str):
        return entry

    parts = entry.split(":")
    source = parts[0]
    if source in named_volumes:
        parts[0] = f"{source}-{node_index}"
        return ":".join(parts)
    return entry


def rewrite_service_volumes(service_name, node_index, volumes, named_volumes):
    if service_name == "router":
        return [
            "./http-only.crt:/etc/nginx/certs/tls.crt:ro",
            "./http-only.key:/etc/nginx/certs/tls.key:ro",
            "./acme-challenge:/var/www/acme-challenge",
        ]

    if not volumes:
        return None

    return [rewrite_volume_entry(entry, node_index, named_volumes) for entry in volumes]


def rewrite_ports(service_name, node_index):
    if service_name == "router":
        return [f"{HTTP_PORT_BASE + node_index}:80"]
    if service_name == "file-system":
        host_port = FILE_SYSTEM_HOST_PORT_ZERO if node_index == 0 else SMB_PORT_BASE + (node_index - 1)
        return [f"{host_port}:445"]
    return None


def image_override_env_name(service_name):
    return f"IMAGE_OVERRIDE_{service_name.upper().replace('-', '_')}"


def maybe_override_image(service_name, image):
    return os.environ.get(image_override_env_name(service_name), image)


def logical_node_name(node_index):
    return f"journal-{node_index}"


def generate_peer_config(node_count, connectivity):
    random.seed(0)
    node_names = [logical_node_name(index) for index in range(node_count)]
    edges = {
        node_name: random.sample(
            [other_name for other_name in node_names if other_name != node_name],
            min(connectivity, len(node_names) - 1),
        )
        for node_name in node_names
    }
    nodes = {
        node_name: {"router_host": f"router-{index}"}
        for index, node_name in enumerate(node_names)
    }
    return {"nodes": nodes, "edges": edges}


def make_social_agent_service(node_index, secret, period, size, activity, words, clients):
    service_name = f"social-agent-{node_index}"
    image = os.environ.get(
        "IMAGE_OVERRIDE_SOCIAL_AGENT",
        f"ghcr.io/sandialabs/sync-analysis/social-agent:{SOCIAL_AGENT_VERSION}",
    )
    return {
        "container_name": service_name,
        "image": image,
        "depends_on": [f"router-{node_index}"],
        "networks": ["public"],
        "environment": {
            "NODE_NAME": logical_node_name(node_index),
            "SECRET": secret,
            "PERIOD": str(period),
            "SIZE": str(size),
            "ACTIVITY": str(activity),
            "WORDS": str(words),
            "CLIENTS": str(clients),
            "PEERS_CONFIG": "/srv/peers.json",
            "BENCHMARK_OUTPUT": "/srv/results/benchmark.json",
        },
        "volumes": [
            "./peers.json:/srv/peers.json:ro",
            f"./metrics/{service_name}:/var/lib/node_exporter/textfile",
            f"./results/{service_name}:/srv/results",
        ],
    }


def make_aggregate_results_service():
    return {
        "container_name": "aggregate-results",
        "image": "python:3.11-alpine",
        "working_dir": "/workspace",
        "command": ["python3", "aggregate_results.py"],
        "volumes": [
            ".:/workspace",
        ],
        "ports": [f"{AGGREGATE_RESULTS_PORT}:8090"],
        "restart": "unless-stopped",
    }


def main():
    base_compose_path = Path(env_required("SYNC_SERVICES_GENERAL_COMPOSE")).resolve()
    node_count = env_int("NODE_COUNT", DEFAULT_NODE_COUNT)
    if node_count <= 0:
        raise SystemExit("NODE_COUNT must be greater than zero")

    secret = env_optional("SECRET", DEFAULT_SECRET)
    connectivity = env_int("CONNECTIVITY", DEFAULT_CONNECTIVITY)
    period = env_int("PERIOD", DEFAULT_PERIOD)
    window = env_int("WINDOW", DEFAULT_WINDOW)
    size = env_int("SIZE", DEFAULT_SIZE)
    activity = env_float("ACTIVITY", DEFAULT_ACTIVITY)
    words = env_int("WORDS", DEFAULT_WORDS)
    clients = env_int("CLIENTS", DEFAULT_CLIENTS)

    base = load_base_compose(base_compose_path)
    base_services = base["services"]
    base_named_volumes = base.get("volumes", {})

    generated = {
        "services": {},
        "networks": {"public": {}},
        "volumes": {},
    }

    METRICS_DIR.mkdir(exist_ok=True)
    RESULTS_DIR.mkdir(exist_ok=True)

    for node_index in range(node_count):
        private_network = f"private-{node_index}"
        generated["networks"][private_network] = {}
        (METRICS_DIR / f"social-agent-{node_index}").mkdir(exist_ok=True)
        (RESULTS_DIR / f"social-agent-{node_index}").mkdir(exist_ok=True)

        for service_name, service in base_services.items():
            generated_name = f"{service_name}-{node_index}"
            generated_service = copy.deepcopy(service)
            generated_service["container_name"] = generated_name
            generated_service.pop("profiles", None)

            if "depends_on" in generated_service:
                generated_service["depends_on"] = rewrite_depends_on(
                    generated_service.get("depends_on"), node_index
                )

            generated_service["environment"] = rewrite_service_environment(
                service_name,
                node_index,
                generated_service.get("environment"),
                secret,
                period,
                window,
            )

            if "image" in generated_service:
                generated_service["image"] = maybe_override_image(
                    service_name, generated_service["image"]
                )

            rewritten_volumes = rewrite_service_volumes(
                service_name,
                node_index,
                generated_service.get("volumes"),
                base_named_volumes,
            )
            if rewritten_volumes is not None:
                generated_service["volumes"] = rewritten_volumes
            else:
                generated_service.pop("volumes", None)

            rewritten_ports = rewrite_ports(service_name, node_index)
            if rewritten_ports is not None:
                generated_service["ports"] = rewritten_ports
            else:
                generated_service.pop("ports", None)

            if service_name in {"journal", "router"}:
                generated_service["networks"] = [private_network, "public"]
            else:
                generated_service["networks"] = [private_network]

            generated["services"][generated_name] = generated_service

        generated["services"][f"social-agent-{node_index}"] = make_social_agent_service(
            node_index, secret, period, size, activity, words, clients
        )

        for volume_name, volume_config in base_named_volumes.items():
            generated["volumes"][f"{volume_name}-{node_index}"] = copy.deepcopy(volume_config)

    peers = generate_peer_config(node_count, connectivity)
    generated["services"]["aggregate-results"] = make_aggregate_results_service()

    SCRIPT_DIR.mkdir(parents=True, exist_ok=True)
    ACME_DIR.mkdir(exist_ok=True)
    HTTP_ONLY_CERT.write_text("HTTP-only compose placeholder cert.\n", encoding="utf-8")
    HTTP_ONLY_KEY.write_text("HTTP-only compose placeholder key.\n", encoding="utf-8")
    OUTPUT_PEERS.write_text(json.dumps(peers, indent=2) + "\n", encoding="utf-8")
    with OUTPUT_COMPOSE.open("w", encoding="utf-8") as fd:
        yaml.safe_dump(generated, fd, sort_keys=False)

    print(f"Wrote {OUTPUT_COMPOSE}")
    print(f"Wrote {OUTPUT_PEERS}")


if __name__ == "__main__":
    main()
