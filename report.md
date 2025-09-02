Static Bandwidth Test – Lack of Packet Flow

    Missing base RIST address/port
    The test builds ristsink with only bonding-addresses; no primary address/port is supplied, so the sink may never initiate a RIST session, resulting in zero traffic

Suggested taskProvide a primary RIST destination in static bandwidth test

All simulated paths share the same interface
Every profile uses the loopback interface ("lo"); applying qdisc parameters in a loop simply overrides the same interface, so only the last profile’s parameters remain active
Suggested taskUse distinct interfaces for each static profile
Network Simulation Library Review

    QdiscManager is a no-op placeholder
    configure_interface only logs and returns success, so no actual traffic shaping occurs; tests relying on real impairments will observe no effect

Suggested taskImplement real qdisc configuration

Documentation promises APIs that do not exist
The README references functions such as remove_network_params, get_interface_stats, and advanced builder patterns that are absent from the codebase
Suggested taskAlign README with available network-sim APIs
Notes

Implementing real traffic shaping will require running tests with CAP_NET_ADMIN; consider tooling or container scripts to set this up automatically.