"""This module contains all necessary Model Component Objects for synchronic_web.social_agent."""

import json

from firewheel.control.experiment_graph import require_class
from linux.ubuntu2204 import Ubuntu2204Server
from utilities.tools import Utilities


@require_class(Utilities)
@require_class(Ubuntu2204Server)
class SocialAgent:
    def __init__(self, journal, secret, period, size, activity, peers, words):
        self.journal = journal
        self.secret = secret
        self.period = period
        self.size = size
        self.activity = activity
        self.peers = peers
        self.words = words

        self.add_docker()
        self.run_agent()
        self.increase_resources()

    def run_agent(self):
        args = " ".join(
            [
                f"-e JOURNAL={self.journal}",
                f"-e SECRET={self.secret}",
                f"-e PERIOD={self.period}",
                f"-e SIZE={self.size}",
                f"-e WORDS={self.words}",
                f"-e ACTIVITY={self.activity}",
                "-v /home/ubuntu/peers.json:/srv/peers.json",
                "-v /home/ubuntu/node-exporter-textfile:/var/lib/node_exporter/textfile",
                "--net=host",
                "--name social-agent",
            ]
        )

        self.drop_file(
            -32,
            "/home/ubuntu/social-agent-image.tar",
            "social-agent-image.tar",
        )
        self.drop_content(
            -31, "/home/ubuntu/peers.json", json.dumps(self.peers, indent=2)
        )
        self.run_executable(
            -31, "mkdir", arguments="-p /home/ubuntu/node-exporter-textfile"
        )
        self.run_executable(
            -30, "docker", "load -i /home/ubuntu/social-agent-image.tar"
        )
        self.run_executable(1, "docker", f"run {args} social-agent")

    def increase_resources(self):
        num_cores = 2
        num_threads = 2
        if self.vm["vcpu"]["cores"] < num_cores:
            self.vm["vcpu"]["cores"] = num_cores
        if self.vm["vcpu"]["threads"] < num_threads:
            self.vm["vcpu"]["threads"] = num_threads
        self.vm["mem"] = 1024 * 4
