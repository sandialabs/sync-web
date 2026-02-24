import os
import sys
import time
import json
import logging
import requests
import threading

from numpy.random import exponential, choice, randint
from datetime import datetime, timedelta
from threading import Thread
from os import environ as env

logger = logging.getLogger(__name__)
logger.addHandler(logging.StreamHandler(sys.stdout))
logger.setLevel(logging.INFO)


DIR = os.path.dirname(os.path.abspath(__file__))

with open(os.path.join(DIR, "frankenstein.txt")) as fd:
    WORDS = "".join(
        x.lower() for x in fd.read() if x.isascii() and x.isalpha() or x.isspace()
    ).split()

NUM_WORDS = int(env['WORDS'])
METRICS_PATH = env.get(
    "METRICS_TEXTFILE", "/var/lib/node_exporter/textfile/social_agent.prom"
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
        self.activity_cycles_success_total = 0

    def record_request(self, function, duration, success):
        with self.lock:
            self.requests_total += 1
            if not success:
                self.requests_failed_total += 1
            if function == "get":
                self.get_latency_sum += duration
                self.get_latency_count += 1
            elif function == "set!":
                self.set_latency_sum += duration
                self.set_latency_count += 1

    def record_cycle(self, success):
        with self.lock:
            self.activity_cycles_total += 1
            if success:
                self.activity_cycles_success_total += 1

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
                "activity_cycles_success_total": self.activity_cycles_success_total,
            }


METRICS = Metrics()


def write_metrics():
    stats = METRICS.snapshot()
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
        "# HELP social_agent_activity_cycles_success_total Total activity cycles where both get and set succeeded.",
        "# TYPE social_agent_activity_cycles_success_total counter",
        f"social_agent_activity_cycles_success_total {stats['activity_cycles_success_total']}",
        "# HELP social_agent_uptime_seconds Uptime of the social agent process.",
        "# TYPE social_agent_uptime_seconds gauge",
        f"social_agent_uptime_seconds {max(time.time() - stats['started'], 0)}",
    ]
    tmp_path = f"{METRICS_PATH}.tmp"
    os.makedirs(os.path.dirname(METRICS_PATH), exist_ok=True)
    with open(tmp_path, "w") as fd:
        fd.write("\n".join(lines))
        fd.write("\n")
    os.replace(tmp_path, METRICS_PATH)


def metrics_writer():
    while True:
        try:
            write_metrics()
        except Exception:
            logger.exception("Failed writing metrics")
        time.sleep(1)


def call(function, *arguments):
    started = time.perf_counter()
    success = False
    try:
        result = requests.post(
            f"http://{env['JOURNAL']}/interface/json",
            json={
                "function": function,
                "arguments": arguments,
                "authentication": env["SECRET"],
            },
        ).json()
        success = True
        logger.info(f"{datetime.now().isoformat()} {function} | {arguments} -> {result}")
        return result
    finally:
        METRICS.record_request(function, time.perf_counter() - started, success)


def run(peers):
    # initialize peering
    for peer in peers[env["JOURNAL"]]:
        # todo: handle public key
        while r := call("general-peer!", peer.rsplit(".", 1)[0], {"*type/string*": f"http://{peer}/interface"}):
            if r is not True:
                logger.warning(f"Could not peer with {peer}, trying again")
                time.sleep(1)
            else:
                break

    # preload the journal
    for i in range(int(env["SIZE"])):
        call(
            "set!",
            [["*state*", "data", f"key-{i}"]],
            {"*type/string*": " ".join(choice(WORDS, NUM_WORDS))},
        )

    # perform a single action
    def _act(call):
        cycle_success = False
        try:
            path, node = [], env["JOURNAL"]
            while choice(2) and peers.get(node):
                node = choice(peers[node])
                path += [-1, ["*peer*", node.rsplit(".", 1)[0], "chain"]]

            path += [-1, ["*state*", "data", f"key-{randint(0, env['SIZE'])}"]]

            # read from the journal
            result = call("get", path)

            if type(result) is not dict or "*type/string*" not in result:
                logger.warning("Cannot complete action")
                return

            ls = result["*type/string*"].split(" ")
            ls[randint(0, NUM_WORDS)] = choice(WORDS)

            # # write to the journal
            set_result = call(
                "set!",
                [["*state*", "data", f"key-{randint(0, env['SIZE'])}"]],
                {"*type/string*": " ".join(ls)},
            )
            cycle_success = set_result is True
        finally:
            METRICS.record_cycle(cycle_success)

    until = datetime.now()
    while True:
        Thread(target=_act, args=[call]).start()
        time.sleep(max((until - datetime.now()).total_seconds(), 0))
        until += timedelta(seconds=float(env["ACTIVITY"]))


if __name__ == "__main__":
    Thread(target=metrics_writer, daemon=True).start()

    # poll until journal is up
    while True:
        try:
            int(call("size"))
            break
        except Exception:
            time.sleep(1)

    with open(os.path.join(DIR, "peers.json")) as fd:
        peers = json.load(fd)

    run(peers)
