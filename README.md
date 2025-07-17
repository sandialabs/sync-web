# The Synchronic Web

The Synchronic Web is a global infrastructure for data assurance, enabling anyone to cryptographically and temporally notarize information. By publishing data to the Synchronic Web, creators and organizations can irrefutably prove the existence and integrity of their data at a specific points in time relative to their trusted anchors. This system supports strong notions of shared global state, provenance, and verifiable history, making it useful for public transparency, cybersecurity, digital media, legal records, intellectual property, and more.

At its core, the Synchronic Web is powered by distributed programs called journals, which maintain immutable, version-controlled logs (records) and continuously synchronize cryptographic metadata with other journals to achieve global consensus.

Please see the full [documentation](https://sandialabs.github.io/sync-web/) for more details.

---

## Repository Contents

This repository serves as the main entry point and documentation hub for the Synchronic Web project. It contains:

- **info/**  
  Documentation, quickstart guides, whitepapers, and additional resources for understanding and using the Synchronic Web.
- **Quick Links to Core Components:**  
  - [Journal SDK](https://github.com/sandialabs/sync-journal): Core executable for deploying a Synchronic Web node ("journal").
  - [Record Logic](https://github.com/sandialabs/sync-records): Lisp/Scheme functions and data structures for configuring journal logic.
  - [Service Deployments](https://github.com/sandialabs/sync-services): Containerized microservices and Docker Compose networks for deploying Synchronic Web applications.
  - [Experiment Analysis](https://github.com/sandialabs/sync-analysis): Experiments for analyzing performance and robustness of Synchronic Web networks.


---
