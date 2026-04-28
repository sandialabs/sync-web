import random

from firewheel.control.experiment_graph import AbstractPlugin, Vertex
from synchronic_web.general_journal import Journal
from synchronic_web.social_agent import SocialAgent

from base_objects import Switch, VMEndpoint
from netaddr import IPNetwork

random.seed(0)


class Plugin(AbstractPlugin):
    """synchronic_web.social_agent plugin documentation."""

    def run(self, connectivity="2", size="32", activity="0", words="8"):
        connectivity = int(connectivity)
        size = int(size)
        activity = float(activity)
        words = int(words)

        journals = [v.name for v in self.g.get_vertices() if v.is_decorated_by(Journal)]

        edges = {
            x: random.sample(
                [y for y in journals if y != x],
                min(connectivity, len(journals) - 1),
            )
            for x in journals
        }
        peers = {
            "nodes": {journal_name: {"router_host": journal_name} for journal_name in journals},
            "edges": edges,
        }

        for vertex in list(self.g.get_vertices()):
            if vertex.is_decorated_by(Journal):
                journal = vertex
                index = int(journal.name.split(".", 1)[0].rsplit("-", 1)[-1])

                # Create the journal/agent network
                journal_agent_net = IPNetwork("10.0.0.0/16")
                ips = journal_agent_net.iter_hosts()
                journal_agent_sw = Vertex(self.g)
                journal_agent_sw.decorate(
                    Switch, init_args=[f"journal-agent-sw-{index}"]
                )

                # Connect the journal VM and its colocated agent on a private network.
                # The agent reaches the local stack through the router exposed on the VM host.
                journal_ip = next(ips)
                journal.connect(journal_agent_sw, journal_ip, journal_agent_net.netmask)

                # Create the agent
                agent_name = f"agent-{index}.net"
                agent = Vertex(self.g, agent_name)
                agent.decorate(
                    SocialAgent,
                    init_args=[
                        journal.name,
                        journal.name,
                        journal.secret,
                        journal.period,
                        size,
                        activity,
                        peers,
                        words,
                    ],
                )

                agent.run_executable(
                    -10,
                    "bash",
                    arguments=f'-c "echo \\"{journal_ip} {journal.name}\\" >> /etc/hosts"',
                    vm_resource=False,
                )

                # Connect the agent to the private network it uses to reach its own router-hosting VM.
                agent.connect(journal_agent_sw, next(ips), journal_agent_net.netmask)
