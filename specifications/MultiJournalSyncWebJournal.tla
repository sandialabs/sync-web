---- MODULE MultiJournalSyncWebJournal ----

(*

[] = always
<> = eventually
! = modify (if ___ EXCEPT ![...], this means that everything is the same, except for ... which has to be modified)
if an action is enabled, it will eventually happen (wf). if it isn't permanently disabled, it will eventually happen (sf).



To use model checking: java -cp tla2tools.jar tlc2.TLC -config MultiJournalSyncWebJournal.cfg MultiJournalSyncWebJournal.tla 
        To check full temporal spec: java -cp tla2tools.jar tlc2.TLC MultiJournalSyncWebJournal.tla    

*)

EXTENDS Integers, Sequences, FiniteSets, TLC, Naturals

CONSTANTS
    Values,
    Paths, 
    MaxWindow, \* max retention window
    MaxIndex, \* max committed history depth
    JournalNames 

ASSUME
    /\ Values # {}
    /\ "" \in Values 
    /\ Paths # {}
    /\ MaxWindow \in Nat \ {0} \* positive window
    /\ MaxIndex \in Nat
    /\ JournalNames # {}

VARIABLES
    history, \* committed journal history
    stage,
    pins, \* set of currently pinned paths
    bridges, \* bridge connections between journals
    config,
    timeCounter, \* time counter
    stepIndex \* latest committed index


IndexSet == 0..MaxIndex

\* creates distinct bridges
BridgeIds == {p \in JournalNames \X JournalNames : p[1] # p[2]}

EmptyValue == ""
EmptyLedger == [p \in Paths |-> EmptyValue] \* every path is empty
EmptyHistory == [k \in IndexSet |-> EmptyLedger] \* map index to empty ledger
MinTraceIndex == 0 - (MaxIndex + 1)
TraceIndexArgs == MinTraceIndex..MaxIndex


vars == <<history, stage, pins, bridges, config, timeCounter, stepIndex>>

\* committed index of journal
CurrentIndex(j) == stepIndex[j]

\* committed ledger of journal
CurrentLedger(j) == history[j][CurrentIndex(j)]



\* convert trace index into history index
ResolveIndex(j, idx) ==
    IF idx >= 0
    THEN idx + 1
    ELSE stepIndex[j] + idx + 1

\* whether a index resolves into a valid snapshot
ValidResolvedIndex(j, idx) ==
    LET k == ResolveIndex(j, idx) IN
        /\ k \in IndexSet
        /\ k >= 0
        /\ k <= stepIndex[j]

WithinWindow(j, k) ==
    /\ k \in IndexSet
    /\ k <= stepIndex[j]
    /\ stepIndex[j] - k < config[j].window

PinnedAt(j, path, k) ==
    k \in pins[j][path]

\* snapshot accessible if it is still retained by a window or pinned
AccessibleSnapshot(j, path, k) ==
    WithinWindow(j, k) \/ PinnedAt(j, path, k)

\* checks if value of path is pinned
IsPinned(j, path) ==
    \E k \in IndexSet : k \in pins[j][path]


StateConstraint ==
    /\ timeCounter <= 12
    /\ \A j \in JournalNames: stepIndex[j] <= MaxIndex

Symmetry == Permutations(Paths) \cup Permutations(JournalNames)

\* empty commit history, empty staged state, no pins, no bridges, max retention window, index at 0
Init ==
    /\ history = [j \in JournalNames |-> EmptyHistory]
    /\ stage = [j \in JournalNames |-> EmptyLedger]
    /\ pins = [j \in JournalNames |-> [p \in Paths |-> {}]]
    /\ bridges = [b \in BridgeIds |->
          [ interface     |-> "",
            valid         |-> FALSE,
            mode          |-> "pull",
            lastSyncIndex |-> 0,
            pushAllowed   |-> FALSE,
            pullAllowed   |-> FALSE ]]
    /\ config = [j \in JournalNames |-> [window |-> MaxWindow]]
    /\ timeCounter = 0
    /\ stepIndex = [j \in JournalNames |-> 0]

time == \* time advancing
    /\ timeCounter' = timeCounter + 1
    /\ UNCHANGED <<history, stage, pins, bridges, config, stepIndex>>

\* commit staged state into new snapshot
step(j) == 
    /\ j \in JournalNames
    /\ stepIndex[j] < MaxIndex
    /\ LET old == stepIndex[j]
           new == old + 1
           nextLedger ==
               [p \in Paths |->
                    IF p = "*state*/*time*"
                    THEN "0" \* or some encoded time token
                    ELSE IF stage[j][p] # EmptyValue
                         THEN stage[j][p]
                         ELSE history[j][old][p]]
       IN
         /\ history' = [history EXCEPT ![j][new] = nextLedger]
         /\ stage' = [stage EXCEPT ![j] = EmptyLedger]
         /\ stepIndex' = [stepIndex EXCEPT ![j] = new]
         /\ timeCounter' = timeCounter + 1
    /\ UNCHANGED <<pins, bridges, config>>


\* ledger and stage operations
resolve(j, idx, path, pinnedOnly, includeProof) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ pinnedOnly \in BOOLEAN
    /\ includeProof \in BOOLEAN
    /\ ValidResolvedIndex(j, idx)
    /\ LET k == ResolveIndex(j, idx) IN
         /\ k \in IndexSet
         /\ k <= stepIndex[j]
         /\ IF k = stepIndex[j]
               THEN history[j][k][path] # EmptyValue \/ stage[j][path] # EmptyValue
               ELSE history[j][k][path] # EmptyValue
    /\ UNCHANGED vars

ledger_pin(j, idx, path) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ ValidResolvedIndex(j, idx)
    /\ LET k == ResolveIndex(j, idx) IN
         /\ history[j][k][path] # EmptyValue
         /\ pins' = [pins EXCEPT ![j][path] = @ \cup {k}]
    /\ UNCHANGED <<history, stage, bridges, config, timeCounter, stepIndex>>

ledger_unpin(j, idx, path) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ ValidResolvedIndex(j, idx)
    /\ LET k == ResolveIndex(j, idx) IN
         /\ k \in pins[j][path]
         /\ pins' = [pins EXCEPT ![j][path] = @ \ {k}]
    /\ UNCHANGED <<history, stage, bridges, config, timeCounter, stepIndex>>

stage_set(j, path, value) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ value \in Values
    /\ stage' = [stage EXCEPT ![j][path] = value]
    /\ UNCHANGED <<history, pins, bridges, config, timeCounter, stepIndex>>

stage_get(j, path) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ stage[j][path] # EmptyValue
    /\ UNCHANGED vars




\* register and update bridge connections
bridge(source, target, interface, info) ==
    /\ <<source, target>> \in BridgeIds
    /\ interface \in Values
    /\ info \in Values
    /\ \/ \E mode \in {"push", "pull"}:
            bridges' = [bridges EXCEPT ![<<source, target>>] = [
                @ EXCEPT
                    !.interface = interface,
                    !.valid = TRUE,
                    !.mode = mode,
                    !.pushAllowed = (mode # "pull"),
                    !.pullAllowed = (mode # "push")
            ]]
       \/ bridges' = [bridges EXCEPT ![<<source, target>>] = [
                @ EXCEPT
                    !.valid = FALSE,
                    !.pushAllowed = FALSE,
                    !.pullAllowed = FALSE
            ]]
    /\ UNCHANGED <<history, stage, pins, config, timeCounter, stepIndex>>




\* push to current stage
bridgePush(source, target) ==
    /\ <<source, target>> \in BridgeIds
    /\ bridges[<<source, target>>].valid
    /\ bridges[<<source, target>>].pushAllowed
    /\ \E path \in Paths:
        CurrentLedger(source)[path] # EmptyValue
        /\ CurrentLedger(source)[path] # CurrentLedger(target)[path]
    /\ stage' =
        [stage EXCEPT ![target] =
            [p \in Paths |->
                IF CurrentLedger(source)[p] # EmptyValue \* if source ledger is not empty and not equal to target ledger,
                   /\ CurrentLedger(source)[p] # CurrentLedger(target)[p]
                THEN CurrentLedger(source)[p]             \* target's stage gets sources's value
                ELSE stage[target][p]]]
    /\ bridges' = [bridges EXCEPT ![<<source, target>>].lastSyncIndex = stepIndex[source]]
    /\ UNCHANGED <<history, pins, config, timeCounter, stepIndex>>

bridgePushNoOp(source, target) == \* when bridge is current, push is no-op
    /\ <<source, target>> \in BridgeIds
    /\ bridges[<<source, target>>].valid
    /\ bridges[<<source, target>>].pushAllowed
    /\ \A path \in Paths:
         CurrentLedger(source)[path] = EmptyValue
         \/ CurrentLedger(source)[path] = CurrentLedger(target)[path]
    /\ bridges' = [bridges EXCEPT ![<<source, target>>].lastSyncIndex = stepIndex[source]]
    /\ UNCHANGED <<history, stage, pins, config, timeCounter, stepIndex>>




\* pull changes intto source's stage
bridgePull(source, target) ==
    /\ <<source, target>> \in BridgeIds
    /\ bridges[<<source, target>>].valid
    /\ bridges[<<source, target>>].pullAllowed
    /\ \E path \in Paths:
        CurrentLedger(target)[path] # EmptyValue
        /\ CurrentLedger(target)[path] # CurrentLedger(source)[path]
    /\ stage' =
        [stage EXCEPT ![source] =
            [p \in Paths |->
                IF CurrentLedger(target)[p] # EmptyValue \* if target ledger is not empty and not equal to source ledger,
                   /\ CurrentLedger(target)[p] # CurrentLedger(source)[p] 
                THEN CurrentLedger(target)[p]            \* source's stage gets target's value
                ELSE stage[source][p]]]
    /\ bridges' = [bridges EXCEPT ![<<source, target>>].lastSyncIndex = stepIndex[source]]
    /\ UNCHANGED <<history, pins, config, timeCounter, stepIndex>>

bridgePullNoOp(source, target) == \* nothing to copy during pulling, gives empty
    /\ <<source, target>> \in BridgeIds
    /\ bridges[<<source, target>>].valid
    /\ bridges[<<source, target>>].pullAllowed
    /\ \A path \in Paths:
         CurrentLedger(target)[path] = EmptyValue
         \/ CurrentLedger(target)[path] = CurrentLedger(source)[path]
    /\ bridges' = [bridges EXCEPT ![<<source, target>>].lastSyncIndex = stepIndex[source]]
    /\ UNCHANGED <<history, stage, pins, config, timeCounter, stepIndex>>




synchronize(id, index) ==
    /\ id \in BridgeIds
    /\ index \in IndexSet
    /\ bridges[id].valid
    /\ \/ (bridges[id].pushAllowed /\ bridgePush(id[1], id[2]))
       \/ (bridges[id].pushAllowed /\ bridgePushNoOp(id[1], id[2]))
       \/ (bridges[id].pullAllowed /\ bridgePull(id[1], id[2]))
       \/ (bridges[id].pullAllowed /\ bridgePullNoOp(id[1], id[2]))
    /\ UNCHANGED <<pins, config, timeCounter, stepIndex>>


Next ==
    \/ \E j \in JournalNames: \E p \in Paths, v \in Values: stage_set(j, p, v)
    \/ \E j \in JournalNames: \E p \in Paths: stage_get(j, p)
    \/ \E j \in JournalNames: \E idx \in TraceIndexArgs, p \in Paths, pinnedOnly \in BOOLEAN, includeProof \in BOOLEAN:
        resolve(j, idx, p, pinnedOnly, includeProof)
    \/ \E j \in JournalNames: \E idx \in TraceIndexArgs, p \in Paths:
        ledger_pin(j, idx, p)
    \/ \E j \in JournalNames: \E idx \in TraceIndexArgs, p \in Paths:
        ledger_unpin(j, idx, p)
    \/ \E id \in BridgeIds: \E interface \in Values, info \in Values:
         bridge(id[1], id[2], interface, info)
    \/ \E id \in BridgeIds: \E index \in IndexSet:
         synchronize(id, index)
    \/ \E j \in JournalNames: step(j)
    \/ \E id \in BridgeIds: bridgePush(id[1], id[2])
    \/ \E id \in BridgeIds: bridgePushNoOp(id[1], id[2])
    \/ \E id \in BridgeIds: bridgePull(id[1], id[2])
    \/ \E id \in BridgeIds: bridgePullNoOp(id[1], id[2])
    \/ time



\* weak fairness
wfstep ==
    \A j \in JournalNames: WF_vars(step(j))

wfset == \* values eventually become committable
    WF_vars(\E j \in JournalNames, p \in Paths, v \in Values: stage_set(j, p, v))

wfresolve == \* paths must eventually resolve
    WF_vars(\E j \in JournalNames, idx \in TraceIndexArgs, p \in Paths:
        resolve(j, idx, p, FALSE, FALSE))

wftime ==
    WF_vars(time)

wfunpin ==
    WF_vars(\E j \in JournalNames, idx \in TraceIndexArgs, p \in Paths:
        ledger_unpin(j, idx, p))

wfbridgePush ==
    \A id \in BridgeIds: WF_vars(bridgePush(id[1], id[2]))

wfbridgePull ==
    \A id \in BridgeIds: WF_vars(bridgePull(id[1], id[2]))

wfsynchronize == \* bridges must eventually synchronize
    WF_vars(\E id \in BridgeIds, index \in IndexSet: synchronize(id, index))



\* strong fairness
sfbridgeSynchronize ==
    SF_vars(\E id \in BridgeIds, index \in IndexSet: synchronize(id, index))

Fairness ==
    /\ wfset
    /\ wfresolve
    /\ wftime
    /\ wfunpin
    /\ wfstep
    /\ wfbridgePush
    /\ wfbridgePull
    /\ wfsynchronize
    /\ sfbridgeSynchronize

Spec ==
    Init /\ [][Next]_vars /\ Fairness




TypeInvariant ==
    /\ history \in [JournalNames -> [IndexSet -> [Paths -> Values]]]
    /\ stage \in [JournalNames -> [Paths -> Values]]
    /\ pins \in [JournalNames -> [Paths -> SUBSET IndexSet]]
    /\ bridges \in [BridgeIds ->
          [ interface     : Values,
            valid         : BOOLEAN,
            mode          : {"push", "pull"},
            lastSyncIndex : Nat,
            pushAllowed   : BOOLEAN,
            pullAllowed   : BOOLEAN ]]
    /\ config \in [JournalNames -> [window : Nat]]
    /\ timeCounter \in Nat
    /\ stepIndex \in [JournalNames -> IndexSet]

WindowConstraints ==
    /\ \A j \in JournalNames:
        /\ config[j].window \in Nat
        /\ config[j].window > 0
        /\ config[j].window <= MaxWindow

BridgeConsistency ==
    /\ \A id \in BridgeIds:
        /\ ~bridges[id].valid => ~bridges[id].pushAllowed /\ ~bridges[id].pullAllowed
        /\ bridges[id].valid => (bridges[id].pushAllowed <=> (bridges[id].mode # "pull"))
        /\ bridges[id].valid => (bridges[id].pullAllowed <=> (bridges[id].mode # "push"))

PinnedPersistence == 
    \A j \in JournalNames:
        \A path \in Paths:
            \A k \in pins[j][path]:
                history[j][k][path] # EmptyValue

stepIndexInvariant ==
    /\ \A j \in JournalNames: stepIndex[j] \in IndexSet \* must be natural
    /\ \A j \in JournalNames: stepIndex[j] <= timeCounter + stepIndex[j] \* less stops than time

SafetyInvariant ==
    /\ TypeInvariant
    /\ BridgeConsistency
    /\ WindowConstraints
    /\ PinnedPersistence
    /\ stepIndexInvariant

THEOREM Spec => []SafetyInvariant


\* liveness properties

\* every bridge becomes eventually become valid
SynchronizationConvergence ==
    \A id \in BridgeIds: []<> (bridges[id].valid = TRUE)

\* committed paths eventually become resolvable, staged value becomes resolvable ledger value
PathAvailability ==
    \A j \in JournalNames:
        \A p \in Paths:
            [] ((stage[j][p] # EmptyValue) ~> (CurrentLedger(j)[p] # EmptyValue))

\* system eventually makes progress in time
ErrorRecovery ==
    []<> (timeCounter > 0)

THEOREM Spec => SynchronizationConvergence
THEOREM Spec => PathAvailability
THEOREM Spec => ErrorRecovery







ResolvedValueNow(j, path) == \* staged value takes over current value
    IF stage[j][path] # EmptyValue
    THEN stage[j][path]
    ELSE CurrentLedger(j)[path]

HasAccessibleHistoryValue(j, path) == \* there exists an accessible value for path
    \E k \in IndexSet :
        /\ k <= stepIndex[j]
        /\ AccessibleSnapshot(j, path, k)
        /\ history[j][k][path] # EmptyValue

PinnedSomewhere(j, path) == \* path has at least one pin
    \E k \in IndexSet : k \in pins[j][path]

\* any path that is within the window or is pinned (and not subsequently unpinned) should be resolvable
SingleJournalAvailability ==
    [] (\A j \in JournalNames : 
        \A path \in Paths : 
            (((\E k \in IndexSet :
                /\ k <= stepIndex[j] /\ AccessibleSnapshot(j, path, k) /\ history[j][k][path] # EmptyValue) \/ PinnedSomewhere(j, path))
                    => HasAccessibleHistoryValue(j, path)))

\* any resolvable path that has been committed always returns the same value
SingleJournalImmutability == 
    [] [\A j \in JournalNames : \A path \in Paths : \A k \in IndexSet :
            (k <= stepIndex[j] /\ history[j][k][path] # EmptyValue
                    => history[j][k][path]' = history[j][k][path])]_vars

\* any path this is resolvable on a single journal and reachable across bridged journals is also resolvable
MultiJournalAvailability ==
    \A id \in BridgeIds :
        \A path \in Paths :
            [] ( (bridges[id].valid /\ CurrentLedger(id[1])[path] # EmptyValue)
                 ~> (CurrentLedger(id[2])[path] # EmptyValue) )

\* any resolvable path through bridged journals that has been committed always returns the same value
MultiJournalImmutability ==
    \A id \in BridgeIds :
        \A path \in Paths :
            [] ( (bridges[id].valid
                  /\ CurrentLedger(id[1])[path] # EmptyValue
                  /\ CurrentLedger(id[2])[path] # EmptyValue)
                 => [] (CurrentLedger(id[2])[path] = CurrentLedger(id[1])[path]) ) 

THEOREM Spec => SingleJournalAvailability
THEOREM Spec => SingleJournalImmutability
THEOREM Spec => MultiJournalAvailability
THEOREM Spec => MultiJournalImmutability

====