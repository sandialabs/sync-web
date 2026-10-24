---- MODULE LedgerTrace ----
(*

Inputs MultiJournalSyncWebJournal.tla with proper traces from LedgerTraceOps.tla

*) 

EXTENDS MultiJournalSyncWebJournal, LedgerTraceOps, Sequences, TLC

VARIABLE traceIndex

vars_t == <<history, stage, pins, bridges, config, timeCounter, stepIndex, traceIndex>>

TraceLen == Len(TraceEvents)

\* bounded checking, not best to use w/ temporal properties
TraceStateConstraint == traceIndex <= TraceLen + 1 

TraceInit ==
    /\ Init
    /\ traceIndex = 1

\* successful bridge registration; MultiJournalSyncWebJournal has nondeterminism with bridge actions
BridgeForced(source, target, interface, info, mode) ==
    /\ <<source, target>> \in BridgeIds
    /\ interface \in Values
    /\ info \in Values
    /\ mode \in {"push", "pull"}
    /\ bridges' =
        [b \in BridgeIds |->
            IF b = <<source, target>>
            THEN [ interface     |-> interface,
                   valid         |-> TRUE,
                   mode          |-> mode,
                   lastSyncIndex |-> bridges[b].lastSyncIndex,
                   pushAllowed   |-> (mode # "pull"),
                   pullAllowed   |-> (mode # "push") ]
            ELSE bridges[b]]
    /\ UNCHANGED <<history, stage, pins, config, timeCounter, stepIndex>>


\* failed bridge registration
BridgeFailForced(source, target) ==
    /\ <<source, target>> \in BridgeIds
    /\ bridges' =
        [b \in BridgeIds |->
            IF b = <<source, target>>
            THEN [ interface     |-> bridges[b].interface,
                   valid         |-> FALSE,
                   mode          |-> bridges[b].mode,
                   lastSyncIndex |-> bridges[b].lastSyncIndex,
                   pushAllowed   |-> FALSE,
                   pullAllowed   |-> FALSE ]
            ELSE bridges[b]]
    /\ UNCHANGED <<history, stage, pins, config, timeCounter, stepIndex>>

DecodeIdx(s) == \* index conversion
    CASE s = "-5" -> -5
      [] s = "-4" -> -4
      [] s = "-3" -> -3
      [] s = "-2" -> -2
      [] s = "-1" -> -1
      [] s = "0"  -> 0
      [] s = "1"  -> 1
      [] s = "2"  -> 2
      [] s = "3"  -> 3
      [] s = "4"  -> 4
      [] s = "5"  -> 5
      [] OTHER    -> 0

ApplyEvent(evt) ==
    CASE evt.op = "stage_set" -> stage_set(evt.j, evt.path, evt.value)
      [] evt.op = "stage_get" -> stage_get(evt.j, evt.path)
      [] evt.op = "resolve" -> resolve(evt.j, DecodeIdx(evt.idx), evt.path, evt.pinnedOnly, evt.includeProof)
      [] evt.op = "ledger_pin" -> ledger_pin(evt.j, DecodeIdx(evt.idx), evt.path)
      [] evt.op = "ledger_unpin" -> ledger_unpin(evt.j, DecodeIdx(evt.idx), evt.path)
      [] evt.op = "step" -> step(evt.j)
      [] evt.op = "bridge" /\ evt.mode = "fail" -> BridgeFailForced(evt.source, evt.target)
      [] evt.op = "bridge" /\ evt.mode # "fail" -> BridgeForced(evt.source, evt.target, evt.interface, evt.info, evt.mode)
      [] evt.op = "bridgePush" -> bridgePush(evt.source, evt.target)
      [] evt.op = "bridgePushNoOp" -> bridgePushNoOp(evt.source, evt.target)
      [] evt.op = "bridgePull" -> bridgePull(evt.source, evt.target)
      [] evt.op = "bridgePullNoOp" -> bridgePullNoOp(evt.source, evt.target)

TraceNext ==
    \/ /\ traceIndex <= TraceLen
       /\ ApplyEvent(TraceEvents[traceIndex])
       /\ traceIndex' = traceIndex + 1
    \/ /\ traceIndex > TraceLen
       /\ UNCHANGED vars_t

TraceSpec ==
    TraceInit /\ [][TraceNext]_vars_t /\ WF_vars_t(TraceNext)

TraceComplete == <>(traceIndex > TraceLen)

====