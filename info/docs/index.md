# Welcome to the Synchronic Web!

The Synchronic Web is a network of information that is locked into a single global view of history.
When nodes publish their data to the Synchronic Web, they gain the ability to irrefutably prove the following statement to the rest of the world: "I commit to this information---and only this information---at this moment in time."
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

The core mechanism that enables access the Synchronic Web is a program called a _journal_.
Other applications can directly read and write data to their local journals to create immutable, version-controlled logs of history called _records_.
Journals continuously exchange cryptographic metadata, mainly hashes, with other journals to maintain global notions of consensus over local state.
By systematically decouple data from its hashes, the Synchronic Web allows entities to decouple the cost of storage from the value of data integrity, enabling new secure ways to share stateful data across network and organizational boundaries.

## Related Work

The Synchronic Web provides a subset of features enabled by conventional public blockchain technology.
Whereas conventional blockchains provide storage and tamper-evidence for application data, the Synchronic Web blockchain only provides tamper-evidence.
However, unlike conventional blockchains, it provides this feature at arbitrary scales and minimal complexity to enable a global network of immutable data.

The Synchronic Web also offers a superset of features enabled by transparency services.
Whereas transparency services assert immutable history owned by a single entity, the Synchronic Web provides a more distributed notion of history that can span multiple organizations.
However, given the closely-aligned objectives, it is likely that the two technologies will prove to be highly convergent and complimentary.

!!! note

    This page is derived from [The Synchronic Web Primer](https://www.osti.gov/biblio/1862729)
