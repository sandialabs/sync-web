# sync-analysis
Synchronic Web Analysis

This project provides analysis tools and simulation components for the Synchronic Web, a distributed ledger system. It includes FIREWHEEL model components for network simulation and Locust-based load testing.

## Components

### FIREWHEEL Model Components

- **general-journal**: Creates journal nodes that maintain distributed ledger state with configurable periodicity and secrets
- **network-monitor**: Provides monitoring and observability using Prometheus and Grafana for real-time network metrics and visualization
- **social-agent**: Simulates social agents that interact with the ledger system, with configurable connectivity, size, and activity parameters

### Load Testing

- **locust**: HTTP load testing using Locust to simulate concurrent users interacting with the ledger system

## Usage

The FIREWHEEL components can be used to create network topologies with journal nodes and social agents for testing distributed ledger behavior at scale. The Locust tests provide performance analysis of the system under load.

## Requirements

- FIREWHEEL simulation framework
- Docker (for containerized components)
- Python 3.x with Locust for load testing
