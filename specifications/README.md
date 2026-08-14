# Setup

`tla2tools.jar` requires Java 11 or later for model checking.

## Required Files

`check-trace.sh` and `run-traced-test.sh` require `journal-sdk`. Build it by running the following comand in the `journal/` directory: 
```sh 
cargo build --release
```

## Running the Trace and Model Checking Workflow

To generate event logs from `test-ledger.scm` and run model checking on the resulting trace, run: 
```sh 
./check-trace.sh
```

## Running the Base TLA+ Model
To run TLC directly on MultiJournalSyncWebJournal.tla with a possible corresponding config file, run: 
```sh 
java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC -workers auto -config MultiJournalSyncWebJournal.cfg MultiJournalSyncWebJournal.tla
```

## Running TLC on the Generated Traces
After check-trace.sh first generates the traces in `check-trace-output/`, it is possible to run TLC on the generated trace specific model with 
```sh 
java -XX:+UseParallelGC -cp ../tla2tools.jar tlc2.TLC -workers auto -config LedgerTrace.cfg LedgerTrace.tla
```

## Folder Layout
| File | Description |
|---|---|
| `check-trace.sh` | Runs the full pipeline: creates trace logs, converts them into modeled TLA+ operations, and runs TLC on the generated trace replay |
| `run-traced-test.sh` | Runs `test-ledger.scm` with tracing to write the extracted event log | 
| `inject_trace.py` | Rewrites ledger creation lines so ledger objects are wrapped with `trace-wrap` | 
| `tracer.scm` | Wraps Scheme objects and logs events for trace extraction |
| `gen_trace.py` | Parses the event log, converts modeled events into TLA+ operations, and generates the TLC input files |
| `MultiJournalSyncWebJournal.tla` | Main TLA+ model of journal state, pinning, bridging, synchronization, and related invariants/properties |
| `MultiJournalSyncWebJournal.cfg` | Small TLC configuration file for checking `MultiJournalSyncWebJournal.tla` |
| `LedgerTrace.tla` | Replays the generated trace operations against `MultiJournalSyncWebJournal.tla` |
| `tla2tools.jar` | The TLA+ Tools file used to run TLC model checking |

## Generated `check-trace-output/` Folder Layout

| File | Description |
|---|---|
| `raw-output.txt` | Complete output from the traced `journal-sdk` run |
| ` trace.log` | The extracted (event ...) records from `raw-output.txt` |  
| `LedgerTraceOps.tla` | Generated TLA+ module containing the sequence of translated trace operations |
| `LedgerTrace.cfg` | Generated TLC configuration file built from the journals, paths, and values found in the traces |
| `tlc-output.txt` | TLC output from model checking the generated trace replay | 