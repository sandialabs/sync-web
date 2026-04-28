from firewheel.control.experiment_graph import AbstractPlugin, Vertex
from synchronic_web.general_journal import Journal

import math
from netaddr import IPNetwork
import networkx as nx
from generic_vm_objects import GenericRouter
from base_objects import Switch, VMEndpoint
from ast import literal_eval


class Plugin(AbstractPlugin):
    """synchronic_web.topology plugin documentation."""

    def split_seq(self, agents, num_agent_routers):
        newseq = []
        splitsize = 1.0 / num_agent_routers * len(agents)
        for i in range(num_agent_routers):
            newseq.append(
                agents[int(round(i * splitsize)) : int(round((i + 1) * splitsize))]
            )
        return newseq

    def run(self, number, period, server="True"):
        num_agents = int(number)
        server_val = literal_eval(server)
        period_val = literal_eval(period)

        max_agents_per_router = 50
        num_routers = math.ceil(num_agents / max_agents_per_router)

        agents = []
        for i in range(num_agents):
            agent = Vertex(self.g, f"journal-{i}.net")
            agent.decorate(
                Journal, init_args=[f"pass-{i}", period_val, server_val]
            )
            agents.append(agent)

        max_bgp_routers = 219
        if (
            num_routers > max_bgp_routers
        ):  # Number of non-reserved and non-private, etc. /8 ranges
            raise ValueError(f"BGP size needs to be {max_bgp_routers} or less")

        # Create the bgp topology
        topology = nx.random_internet_as_graph(num_routers)
        assert nx.is_connected(topology)

        # Define the networks
        self.control_net = IPNetwork("192.168.0.0/8")
        control_nets = self.control_net.subnet(20)
        self.host_nets = IPNetwork("0.0.0.0/0").subnet(16)
        as_nums = iter(range(1, num_routers + 1))

        # In format {<name> : [<ip address>]} ********
        self.host_ip_map = {}

        # Divvy up agents according to how many BGP routers there will be.
        agent_endpoint_distribution = self.split_seq(agents, num_routers)
        leaves = []  # will be a list of dicts where the index is router number. Dicts contain leaf, bgp_net, switch, bgp_ips

        # Build every BGP router that we want. Connect each BGP router to an
        # OSPF router. Connect agents to all OSPF routers.
        for i in range(num_routers):
            hosts_to_add = agent_endpoint_distribution[i - 1]

            bgp_router = self._make_router_pair(
                "leaf-%d.net" % (i,), control_nets, as_nums, hosts_to_add
            )

            bgp_net = next(control_nets)
            leaves.append(
                {
                    "bgp_router": bgp_router,
                    "bgp_ips": bgp_net.iter_hosts(),
                    "bgp_netmask": bgp_net.netmask,
                }
            )

        # neighbor_list = {
        #    'bgp.leaf-1.net': {
        #        'bgp.leaf-2.net': '192.0.1.1',
        #        'bgp.leaf-3.net': '192.0.2.1'
        #    }
        # }
        neighbor_list = {}
        for x in range(num_routers):
            neighbor_list["bgp.leaf-{}.net".format(x)] = {}

        # Combine the leaves into a single network by taking the BGP router
        # in each leaf and connecting them according to the given topology.
        for i, obj in enumerate(leaves):
            bgp_router = obj["bgp_router"]
            bgp_netmask = obj["bgp_netmask"]
            bgp_ips = obj["bgp_ips"]
            switch_exists = False

            for edge in topology.edges:
                if edge[0] != i:
                    continue

                if not switch_exists:
                    switch = Vertex(self.g)
                    switch.decorate(Switch, init_args=["root-leaf%d.switch" % (i,)])
                    switch_exists = True
                    ip = next(bgp_ips)
                    bgp_to_switch_ip = ip
                    bgp_router.connect(switch, ip, bgp_netmask)
                    self.host_ip_map.setdefault(bgp_router.name, []).append(ip)

                connect_obj = leaves[edge[1]]
                ip = next(bgp_ips)
                neighbor_list[bgp_router.name][connect_obj["bgp_router"].name] = str(ip)
                neighbor_list[connect_obj["bgp_router"].name][bgp_router.name] = str(
                    bgp_to_switch_ip
                )
                connect_obj["bgp_router"].connect(switch, ip, bgp_netmask)
                self.host_ip_map.setdefault(connect_obj["bgp_router"].name, []).append(
                    ip
                )

                bgp_router.link_bgp(connect_obj["bgp_router"], switch, switch)

            # Ensure the bgp_router got an interface
            assert len(bgp_router.interfaces.interfaces) != 0

        # Assign etc hosts values to each endpoint
        vm_endpoints = [
            v for v in self.g.get_vertices() if v.is_decorated_by(VMEndpoint)
        ]

        # assign etc hosts
        sorted_keys = sorted(self.host_ip_map.keys())
        etc_hosts_list = []
        for key in sorted_keys:
            for ip in self.host_ip_map[key]:
                etc_hosts_list.append("{} {} {}".format(ip, key, key.split(".")[0]))
        etc_hosts = "\n".join(etc_hosts_list)

        etc_hosts += "\n"
        for vertex in self.g.get_vertices():
            if vertex.is_decorated_by(Journal):
                vertex.drop_content(-50, "/tmp/hosts", etc_hosts)
                vertex.run_executable(-45, "cat", "/tmp/hosts >> /etc/hosts")
                assert len(vertex.interfaces.interfaces) != 0, (
                    "Vertex {} had no interfaces".format(vertex.name)
                )

    def _next_net(self):
        for network in self.host_nets:
            if self.confirm_valid_agent_network(network):
                return network

    def confirm_valid_agent_network(self, network):
        """
        Confirm that a network is a valid network to assign an agent.

        Check the given network against a variety of criteria that could
        make it an invalid network for assigning to an agent. This could
        include being a network of private IPV4 addresses, reserved
        addresses, loopback addresses or FIREWHEEL control network
        addresses.

        Args:
            network (netaddr.IPNetwork): The network to be checked.

        Returns:
            bool: Whether or not the network is a nonlocal (valid)
            network.
        """
        firewheel_control_net = IPNetwork("172.16.0.0/16")
        invalidating_criteria = [
            network.network.is_ipv4_private_use(),
            network.is_reserved(),
            network.is_link_local(),
            network.is_loopback(),
            network.is_multicast(),
            network in firewheel_control_net,
            network in self.control_net,
        ]
        return not any(invalidating_criteria)

    def _set_router_resources(self, router):
        try:
            router.vm["mem"] = 1024
        except AttributeError:
            router.vm = {"mem": 1024}

        router.vm["vcpu"] = {"sockets": 2}

    def _make_router_pair(self, name, control_nets, as_nums, agents_to_connect):
        """Internal function to create the host, OSPF, BGP sequence"""
        max_agents_per_router = 255
        if len(agents_to_connect) > max_agents_per_router:
            raise ValueError(
                f"Too many agents per router, max is {max_agents_per_router}. Please add another BGP router."
            )

        ospf_net = next(control_nets)
        ospf_ips = ospf_net.iter_hosts()

        ospf = Vertex(self.g, "ospf.%s" % (name,))
        ospf.decorate(GenericRouter)
        self.add_vyos_profiles(ospf)
        self._set_router_resources(ospf)

        as_num = next(as_nums)
        bgp = Vertex(self.g, "bgp.%s" % (name,))
        bgp.decorate(GenericRouter)
        self.add_vyos_profiles(bgp)
        bgp.set_bgp_as(as_num)
        self._set_router_resources(bgp)

        def _new_switch(name, i):
            switch_h_o = Vertex(self.g)
            sw_name = f"switch-host-ospf-{i}.{name}"
            switch_h_o.decorate(Switch, init_args=[sw_name])
            return switch_h_o

        def _get_slash_8(net):
            sp = str(net).split(".")
            sp[1] = "0"
            net = ".".join(sp)
            sp = net.split("/")
            sp[1] = "8"
            net = "/".join(sp)
            return IPNetwork(net)

        # Assign each agent its own /16 address, but keep everything in a /8 on
        # the same switch to help with VLAN limits
        prev_net = None
        switch_h_o = _new_switch(name, 0)
        netmask = "255.0.0.0"
        host_net = None
        for i, agent in enumerate(agents_to_connect):
            host_net = self._next_net()

            host_ips = host_net.iter_hosts()

            host_ip = next(host_ips)
            self.host_ip_map.setdefault(agent.name, []).append(host_ip)
            agent.connect(switch_h_o, host_ip, netmask)

        if host_net is None:
            host_net = self._next_net()
            host_ips = host_net.iter_hosts()
        slash_8 = _get_slash_8(host_net)
        ospf_ip = next(host_ips)
        ospf.connect(switch_h_o, ospf_ip, netmask)

        slash_8 = _get_slash_8(host_net)
        while self._next_net() in slash_8:
            # Don't need to do anything, just increment the generator
            pass

        # Switch to connect ospf router to bgp router
        switch_o_b = Vertex(self.g)
        switch_o_b.decorate(Switch, init_args=["switch-ospf-bgp.%s" % (name,)])

        # Connect ospf and bgp router
        ospf.ospf_connect(switch_o_b, next(ospf_ips), ospf_net.netmask)
        ip = next(ospf_ips)
        bgp.ospf_connect(switch_o_b, ip, ospf_net.netmask)
        self.host_ip_map.setdefault(bgp.name, []).append(ip)

        ospf.redistribute_ospf_connected()
        bgp.redistribute_bgp_into_ospf()
        bgp.redistribute_ospf_into_bgp()

        return bgp

    def add_vyos_profiles(self, vyos_object):
        # root
        vyos_object.drop_file(
            -249,
            "/root/combined_profiles.tgz",
            "combined_profiles.tgz",
        )
        vyos_object.run_executable(
            -248,
            "chown",
            "-R root:root /root/combined_profiles.tgz",
            vm_resource=False,
        )
        vyos_object.run_executable(
            -247,
            "tar",
            "--no-same-owner -C /root/ -xf /root/combined_profiles.tgz",
        )
        vyos_object.run_executable(
            -246,
            "rm",
            "-f /root/combined_profiles.tgz",
        )

        # vyos
        vyos_object.drop_file(
            -249,
            "/home/vyos/combined_profiles.tgz",
            "combined_profiles.tgz",
        )
        vyos_object.run_executable(
            -248,
            "chown",
            "-R vyos:vyos /home/vyos/combined_profiles.tgz",
            vm_resource=False,
        )
        vyos_object.run_executable(
            -247,
            "su",
            'vyos -c "tar -C /home/vyos -xf /home/vyos/combined_profiles.tgz"',
        )
        vyos_object.run_executable(
            -246,
            "rm",
            "-f /home/vyos/combined_profiles.tgz",
        )
