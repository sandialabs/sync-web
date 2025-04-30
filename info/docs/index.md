# Welcome to the Synchronic Web!

The Synchronic Web is a network of information that is locked into a single global view of history.
When clients notarize their data to the Synchronic Web, they gain the ability to irrefutably prove the following statement to the rest of the world: "I commit to this information---and only this information---at this moment in time."
Much like encryption or digital signatures, this capability has the potential to bolster the integrity of public cyberspace at a foundational level and scale.

## Use Cases

The following are examples of domains that can benefit from the Synchronic Web:

* __Public data__: Governments and research groups notarize data for public transparency.
* __Cybersecurity__: Computer systems notarize activity logs for forensics.
* __Digital media__: Social and traditional media outlets notarize content for public consumption.
* __Legal__: Legal entities notarize files and documents for court proceedings.
* __Intellectual property__: Artists and inventors notarize original works for creative credit.
* __Financial technology__: Financial institutions notarize transactions for auditing.
* __Supply chains__: Merchants notarize inventory for customers and regulators.
* __Cryptocurrencies__: Validator nodes notarize blocks for more secure and timely consensus.

Traditional public key infrastructure enables the verifier in each of these uses cases to link data to specific identities in cyberspace.
The Synchronic Web further enables verifiers to link data to specific points in time.
Together, these cryptographic primitives constrain the space of disinformation that adversaries can convincingly fabricate. 

## Architecture

The core of the Synchronic Web is a small, simple blockchain used to synchronize the state of the web.
A consortium of well-known organizations publish new blocks by operating nodes in a fault-tolerant consensus protocol.
Any client in the world can send notarization requests in the form of a key/value pair.
The key is linked to their cryptographic identity and the value is a confidential fingerprint of their data.
Using a simple but novel encoding technique, the blockchain network processes these requests and returns proofs to each client showing that they committed to one string of data---and only one string---at that moment in time.

Clients, which can be implemented as plugins or microservices, must conform to a basic set of semantic specifications.
They can support a wide variety of applications including centralize databases, nodes in another decentralized blockchain, browser extensions to verify web content, or graphical interfaces for committing user-uploaded files.

## Related Work

The Synchronic Web provides a subset of features enabled by conventional public blockchain technology.
Whereas conventional blockchains provide storage and tamper-evidence for application data, the Synchronic Web blockchain only provides tamper-evidence.
However, unlike conventional blockchains, it provides this feature at arbitrary scales and minimal complexity to enable a global network of immutable data.

The Synchronic Web also offers a more secure and trustworthy alternative to the existing timestamping authority infrastructure.
Today, centralized timestamping authorities provide adequate service for personal uses like emails, e-Signatures or Electronic Lab Notebooks.
The Synchronic Web extends this infrastructure with stronger security and trust guarantees that are uniquely suitable for highly consequential, highly controversial, or multi-national use cases.

!!! note

    This page is derived from [The Synchronic Web Primer](https://www.osti.gov/biblio/1862729)
