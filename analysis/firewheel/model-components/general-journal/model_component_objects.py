"""This module contains all necessary Model Component Objects for synchronic_web.journal."""

from firewheel.control.experiment_graph import require_class
from linux.ubuntu2204 import Ubuntu2204Server, Ubuntu2204Desktop
from utilities.tools import Utilities


@require_class(Utilities)
class Journal:
    def __init__(self, secret, period, server=True):
        if server:
            self.decorate(Ubuntu2204Server)
        else:
            self.decorate(Ubuntu2204Desktop)

        self.secret = secret
        self.period = period
        self.add_docker()
        self.run_journal()

        num_cores = 2
        num_threads = 2
        if self.vm["vcpu"]["cores"] < num_cores:
            self.vm["vcpu"]["cores"] = num_cores
        if self.vm["vcpu"]["threads"] < num_threads:
            self.vm["vcpu"]["threads"] = num_threads
        self.vm["mem"] = 2**12

    def run_journal(self):
        env = " ".join(
            [
                "PORT=80",
                f"SECRET={self.secret}",
                f"PERIOD={self.period}",
                "WINDOW=128",
            ]
        )

        self.drop_file(-32, "/home/ubuntu/general-images.tar", "general-images.tar")
        self.drop_file(-32, "/home/ubuntu/general-compose.tar", "general-compose.tar")
        self.run_executable(
            -31, "bash", arguments='-c "cd /home/ubuntu && tar -xf general-images.tar"'
        )
        self.run_executable(
            -31, "bash", arguments='-c "cd /home/ubuntu && tar -xf general-compose.tar"'
        )
        self.run_executable(
            -30,
            "bash",
            '-c "cd /home/ubuntu/general-images && for f in *; do docker load -i \\$f; done"',
        )
        self.run_executable(
            1,
            "bash",
            arguments=f'-c "cd /home/ubuntu/general-compose && {env} docker compose up"',
        )


@require_class(Utilities)
class Monitor:
    def __init__(self, server=True):
        if server:
            self.decorate(Ubuntu2204Server)
        else:
            self.decorate(Ubuntu2204Desktop)

        self.run_monitor()

        num_cores = 2
        num_threads = 2
        if self.vm["vcpu"]["cores"] < num_cores:
            self.vm["vcpu"]["cores"] = num_cores
        if self.vm["vcpu"]["threads"] < num_threads:
            self.vm["vcpu"]["threads"] = num_threads
        self.vm["mem"] = 2**12

    def run_journal(self):
        env = " ".join(
            [
                "PORT=80",
            ]
        )

        self.drop_file(-32, "/home/ubuntu/monitor-images.tar", "monitor-images.tar")
        self.drop_file(-32, "/home/ubuntu/monitor-compose.tar", "monitor-compose.tar")
        self.run_executable(
            -31, "bash", arguments='-c "cd /home/ubuntu && tar -xf monitor-images.tar"'
        )
        self.run_executable(
            -31, "bash", arguments='-c "cd /home/ubuntu && tar -xf monitor-compose.tar"'
        )
        self.run_executable(
            -30,
            "bash",
            '-c "cd /home/ubuntu/monitor-images && for f in *; do docker load -i \\$f; done"',
        )
        self.run_executable(
            1,
            "bash",
            arguments=f'-c "cd /home/ubuntu/monitor-compose && {env} docker compose up"',
        )
