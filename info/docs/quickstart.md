# Quickstart

The Synchronic Web is infrastructure for affording data assurance.
By invoking the infrastructure, data publishers/creators make it possible for data consumers to cryptographically/temporally verify content, thereby ensuring its integrity and facilitating a strong notion of shared global state.
More advanced invocation of the infrastructure makes it possible for data consumers to semantically verify change in data over time, thereby facilitating a strong notion of data provenance.
This document describes ways to invoke Synchronic Web infrastructure to achieve these goals.

## Deploy

The Synchronic Web journals can be deployed in a variety of different environments.
For convenience, we provide some default deployments in the form of docker compose environments such as the _ledger_ deployment.

To run this journal, clone the [services repository](https://github.com/sandialabs/sync-services), and follow the instructions under `compose/ledger/README.md`.
If successful, you should now have access to the following endpoints:

- `http://localhost:<port>`: GUI for basic interactions with the ledger journal
- `http://localhost:<port>/.interface`: API for full control of the ledger journal

The remainder of this document will refer to the API, which can either be accessed from the web browser input box or any commandline tool that can send POST requests to the same endpoint.

## Key-Value Store 

The most simplistic usage of Synchronic Web ledger journal is as a key-value store.
For example, say we write an article called “The Synchronic Web,” whose pdf sha256 hashes to `116677764D9AADB6614F4D4512886466025A56F75A9A5DBEB32B8D609513E236`, and we publish it to arxiv.org on 25 Jan 2023.

Coincident with publication, we can invoke the journal sdk via: 

`(*local* "password" (ledger-set! (*state* documents arxiv the-synchronic-web) "0x116..."))`

`> #t`

And then, any time in the future, we can query the journal to retrieve the hash, in order to compare it with a value you compute yourself from the article pdf.

`(*local* "password" (ledger-get (*state* documents arxiv the-synchronic-web))`

`> "0x116..."`

Even though this is a simple example, it already illustrates a few useful commands.
First, note the boilerplate that prefixes the actual operation with keyword `*local*`, a `"password"`.
This is for authentication purposes: since the entrypoint to Synchronic Web journals is exposed to the network, the ledger interface implements its own authentication constraints so that only "local" entities (i.e., ones that know the root password) can write to the ledger.
For privileged "remote" operations, we would need to specify more sophisticated cryptographic authentication, which the journal does support behind the scenes.
Next, notice that the "key" isn't so much a single term as it is a list of terms that form a path to the value.
You can think of this path as similar to a URL or filesystem path in the sense that it facilitates logical groupings of keys.
Finally, although the "value" that we store in this example is a simple hex string, in reality it can be any valid Scheme expression such as boolean, a number, or even an arbitrary structure of nested lists.

## Immutable State

In addition to key-value storage, the ledger interface also supports immutable version of changing data over time, .e., state.
You may have noticed that, by default, the Docker Compose deployment automatically "steps" (versions) the journal periodically.
You can see the latest step number, which we refer to as an index, like so:

`(*local* "password" (ledger-index))`

`> 1`

This index represent relative notions of time on the Synchronic Web. 
Consider an example of a naturally changing data artifact: a software repository.
We can imagine setting up a git CI/CD pipeline that automatically writes to a journal with every git push like so:

`(*local* "password" (ledger-set! (*state* repos my-repo) "0x32a8bcdd...32"))`

`> #t`

Then, 10 seconds later, we might write a new commit push to the same path: 

`(*local* "password" (ledger-set! (*state* repos my-repo) "0xda3ff8d1...1b"))`

`> #t`

Using the standard ledger interface, we can query for different versions of the same data using a second "index" argument to distinguish between time steps:

`(*local* "password" (ledger-get (*state* repos my-repo) 2))`

`> "0x32a8bcdd...32"`

`(*local* "password" (ledger-get (*state* repos my-repo) 10))`

`> "0xda3ff8d1...1b"`

For convenience, the interface also supports negative indexing to refer to more recent timesteps:

`(*local* "password" (ledger-get (*state* repos my-repo) -1))`

`> "0xda3ff8d1...1b"`

While it is possible to configure the interface to store historical state indefinitely, it is often preferable to delete historical states after some number of indices have elapsed in the interest of space.
For these deployments, you can "pin" historical data that you wish to persist:

`(*local* "password" (ledger-pin! (*state* repos my-repo) 8)`

`> #t`

You can also "unpin" it later:

`(*local* "password" (ledger-unpin! (*state* repos my-repo) 8)`

`> #t`

In this way, Synchronic Web journals provide an easy way to track the change of important information over time.
The datastore is said to be "immutable" because new data extends, rather than replaces, old data.
However, this assumes honest behavior by everyone with access to the journal--there is no strong mechanism to prevent a sophisticated adversary from altering the database to fabricate false versions of history.
To mitigate this possibility, we require a more distributed setup.

## Distributed Ledger

The core capability of Synchronic Web journals--i.e., the capability that motivates its development and enables all distinguishing use cases--is the ability to securely share state across distributed systems.
The objective is for users to be able to read and write historical state from any connected Synchronic Web journal _as if_ it came from a single file system.

For example, consider a four-journal network where each journal is controlled by a different organization in a manufacturing supply chain:

[Journal 1] ---> [Journal 2] ---> [Journal 3]
                      |
                      |---------> [Journal 4]


The ledger interface allows us to concretely build this network through the directed _peering_ functionality.
On each journal:

`(*local* "password-1" (ledger-peer! journal-2 (lambda (msg) (sync-remote "http://journal-2.io" msg)))) ; on Journal 1`

`(*local* "password-2" (ledger-peer! journal-3 (lambda (msg) (sync-remote "http://journal-3.io" msg)))) ; on Journal 2`

`(*local* "password-2" (ledger-peer! journal-4 (lambda (msg) (sync-remote "http://journal-4.io" msg)))) ; on Journal 2`

In the `ledger-peer!` function, the first argument is the name of the peer, which the journal is free to chose.
The second argument is the logic for a function that the ledger interface uses to communicate with the remote journal.
Allowing an arbitrary message-passing function is useful for supporting bespoke networking and deployment environments. 
However, for most use cases, the simple function provided in this example will suffice.

Once a peering relationship is established, the upstream journal will send periodic synchronization requests for the latest cryptographic hashes representing the state of the downstream peer.
Later on, the upsteam journal can use this lightweight check to securely verify any future, more substantial state it needs from the peer.

For example, suppose that nodes 3 and 4 update their local states about 10 seconds after they come online.

`(*local* "password-3" (ledger-set! (*state* status sensor-a) online)) ; on Journal 3`

`(*local* "password-4" (ledger-set! (*state* status sensor-b) failed)) ; on Journal 4`

Sometime later, suppose that the organization controlling Journal 1 discovers it is has been attacked by an advanced threat actor and needs to check the historical health of its sensors.
Using the commands queries, it can securely query for the necessary data:

`(*local* "password-1" (ledger-get (*peers* journal-2 *peers* journal-3 *state* status sensor-a)) 12) ; on Journal 1`

`> online`

`(*local* "password-1" (ledger-get (*peers* journal-2 *peers* journal-4 *state* status sensor-b)) 12) ; on Journal 1`

`> failed`

The returned values are, in a crytographically rigorous sense, secure.
Whereas an adversary might be able to compromise a single node, it is compoundingly more difficult for it to compromise all nodes in a distributed network.
To convincingly fake Journal 4's sensor status, for example, the adversary would have to alter all of Journal 4, Journal 2 and Journal 1's databases.
Failure to synchronize the entire attack would cause an overt data integrity alert.

Finally, much like with local state, it is also possible to pin and unpin states from remote journals.
Here, pinning can both reduce network fetching time and also preserve verifiable proof of the result indefinitely.

`(*local* "password" (ledger-pin! (*peers* journal-2 *peers* journal-3 *state* status sensor-a)) 12)`

`(*local* "password" (ledger-unpin! (*peers* journal-2 *peers* journal-4 *state* status sensor-b)) 12)`

Between network communications and cryptographic data structure operations, there is a lot going on behind the scenes.
However, the end result is exceedingly simple: the ability to use a single path to navigate across time and space to any point in the network.
Thanks to the properties of cryptographic hashes there is no limit to the amount of data or the number of journals that we can eventually be integrated.

## Arbitrary Computation

Using this shared state, the vision for the Synchronic Web is to enable arbitrarily extensible computation an unbounded amount of shared data.
In some sense, arbitrary computations are already possible; all of the ledger functionality that has been described so far are, in fact, modular applications of the same Lisp/Scheme that the journal makes available for extensibility.
Future development can therefore build on this ledger interface to support some combination of the following functionality:

* Constrained reads: ensure that sensitive data on the ledger is access-controlled
* Constrained writes: ensure that data on the ledger are well-formed
* Derived reads: create higher-level views of data on the ledger to support downstream usage
* Derived writes: create arbitrary writes to the ledger based on user input or periodic jobs 

By supporting such general computation on the same globally shared data structure, the Synchronic Web affords new ways to establish security and trust across the digital landscape.
