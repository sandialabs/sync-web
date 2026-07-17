---- MODULE MultiJournalSyncWebJournal ----

(*

[] = always
<> = eventually
! = modify (if ___ EXCEPT ![...], this means that everything is the same, except for ... which has to be modified)
if an action is enabled, it will eventually happen (wf). if it isn't permanently disabled, it will eventually happen (sf).



To use model checking: java -cp tla2tools.jar tlc2.TLC -config SyncWebJournal.cfg SyncWebJournal.tla 
        To check full temporal spec: java -cp tla2tools.jar tlc2.TLC SyncWebJournal.tla    
*)

EXTENDS Integers, Sequences, FiniteSets, TLC, Naturals

CONSTANTS
    Values,
    Paths,
    MaxWindow,
    MaxIndex,
    JournalNames

ASSUME
    /\ Values # {}
    /\ MaxWindow \in Nat \ {0}
    /\ Paths # {}
    /\ MaxIndex \in Nat
    /\ JournalNames # {}

VARIABLES
    ledger,     \* Mapping from paths to committed values
    stage,
    pins,   \* Set of currently pinned paths
    bridges,    \* Bridge connections to other journals
    config,     \* Ledger configuration (window size)
    timeCounter,    \* Time counter
    committed,
    stepIndex,
    windowPosition,
    temp,   \* Temporary chain for recent states (within window)
    perm,    \* Permanent chain for all committed states
    tempIndex


\* StateConstraint: bounds the two unbounded Nat counters so TLC terminates.
StateConstraint == \* safety invariant uses (8,6). liveness uses (12,10)
    /\ timeCounter <= 8
    \* /\ stepIndex <= 6
    \* /\ windowPosition <= MaxIndex
    /\ \A j \in JournalNames: stepIndex[j] <= 6
    /\ \A j \in JournalNames: windowPosition[j] <= MaxIndex

\* Symmetry == Permutations(Paths)
Symmetry == Permutations(Paths) \cup Permutations(JournalNames)

IndexSet == 0..MaxIndex
\* BridgeNames == {"bridge1", "bridge2"}
BridgeIds == {p \in JournalNames \X JournalNames : p[1] # p[2]}

EmptyValue == ""
EmptyLedger == [path \in Paths |-> EmptyValue]
\* IsEmptyLedger(l) == \A p \in Paths: l[p] = EmptyValue

(* State Variables
ledger - Mapping from paths to committed values 
pins - Set of currently pinned paths that must remain available
bridges - Registered bridge connections to other journals
config - Ledger configuration (window size)
timeCounter - Monotonic time counter for temporal consistency
committed - represents the committed state of the ledger, contains all paths that ahve been permanently recorded
stepIndex - progression of system, increasing counter
windowPosition - tracks current position of sliding window in chain
temp - stores recent states within the sliding window
perm - stores all committed states *)
vars == <<ledger, stage, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

time ==
    /\ timeCounter' = timeCounter + 1
    /\ UNCHANGED <<ledger, stage, pins, bridges, config, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

IsPinned(j, path) == path \in pins[j]

TypeInvariant == 
    /\ ledger \in [JournalNames -> [Paths -> Values]] \* Ledger maps paths to values
    /\ stage \in [JournalNames -> [Paths -> Values]]
    /\ \A j \in JournalNames: \A path \in Paths: stage[j][path] \in Values  \*all stage values must be in values set
    /\ \A j \in JournalNames: \A path \in Paths: ledger[j][path] \in Values
    /\ pins \in [JournalNames -> SUBSET Paths] \* pins are subset of paths
    /\ bridges \in [BridgeIds -> [
        interface : Values,
        valid : BOOLEAN,
        mode : {"push", "pull"}, 
        lastSyncIndex : Nat, \* Track last time synchronized
        pushAllowed : BOOLEAN, \* push to bridge
        pullAllowed : BOOLEAN
        ]]  \* for every path, there has to be some defined value. remoteledger function must be defined for all paths
    /\ config \in [JournalNames -> [window : Nat]] \* Config contains window size
    /\ timeCounter \in Nat
    /\ committed \in [JournalNames -> [Paths -> Values]]
    /\ \A j \in JournalNames: \A path \in Paths: committed[j][path] \in Values \* All committed values must be in Values set
    /\ stepIndex \in [JournalNames -> Nat]
    /\ windowPosition \in [JournalNames -> IndexSet]  \* Ensure windowPosition stays within bounds
    /\ temp \in [JournalNames -> [Paths -> Values]]   \* Temporary chain stores ledger snapshots, recent states in sliding window
    /\ \A j \in JournalNames: \A path \in Paths: temp[j][path] \in Values
    /\ perm \in [JournalNames -> [Paths -> Values]] \* Permanent chain stores ledger snapshots, immutable history (pin)
    /\ \A j \in JournalNames: \A path \in Paths: perm[j][path] \in Values
    /\ tempIndex \in [JournalNames -> [Paths -> IndexSet]]

Init ==
    /\ ledger = [j \in JournalNames |-> [p \in Paths |-> EmptyValue]]  \* Empty ledger initially
    /\ stage = [j \in JournalNames |-> [p \in Paths |-> EmptyValue]]
    /\ committed = [j \in JournalNames |-> [p \in Paths |-> EmptyValue]]
    /\ pins = [j \in JournalNames |-> {}]                             \* No pinned paths initially
     /\ bridges = [b \in BridgeIds |-> [
        interface |-> "",
        valid |-> FALSE,
        mode |-> "pull",
        lastSyncIndex |-> 0,
        pushAllowed |-> FALSE,
        pullAllowed |-> FALSE \* Invalid bridges cannot allow operations
        ]]
    /\ config = [j \in JournalNames |-> [window |-> MaxWindow]]
    /\ timeCounter = 0 
    /\ stepIndex = [j \in JournalNames |-> 0]
    /\ windowPosition = [j \in JournalNames |-> 0]
    /\ temp = [j \in JournalNames |->  EmptyLedger]
    /\ perm = [j \in JournalNames |-> EmptyLedger]
    /\ tempIndex = [j \in JournalNames |-> [p \in Paths |-> 0]]


\* index gets incremented in step, stage gets committed to ledger (committing happens here)
step(j) == 
    /\ j \in JournalNames
    /\ windowPosition[j] < MaxIndex
    /\ ledger' = [ledger EXCEPT ![j] = [p \in Paths |-> IF stage[j][p] # EmptyValue THEN stage[j][p] ELSE ledger[j][p]]]
    /\ committed' = [committed EXCEPT ![j] = ledger'[j]]
    /\ stage' = [stage EXCEPT ![j] = [p \in Paths |-> EmptyValue]]
    /\ windowPosition' = [windowPosition EXCEPT ![j] = windowPosition[j] + 1]
    /\ temp' = \*
         [temp EXCEPT ![j] = [p \in Paths |->
            IF stage[j][p] # EmptyValue \* committed
            THEN ledger'[j][p] 
            ELSE IF windowPosition'[j] - tempIndex[j][p] <= config[j].window \* still within window
                THEN temp[j][p]
                ELSE EmptyValue]] \* left temp
    /\ perm' = perm
    /\ stepIndex' = [stepIndex EXCEPT ![j] = stepIndex[j] + 1]
    /\ timeCounter' = timeCounter + 1
    /\ tempIndex' = 
         [tempIndex EXCEPT ![j] = [p \in Paths |-> 
            IF stage[j][p] # EmptyValue
            THEN windowPosition'[j] 
            ELSE tempIndex[j][p]]]
    /\ UNCHANGED <<pins, bridges, config>>
    
IsWithinWindow(j, path) == temp[j][path] # EmptyValue


\* ledger and stage operations

resolve(j, path, pinnedOnly, includeProof) ==
    /\ j \in JournalNames
    /\ path \in Paths \* Path must be valid
    /\ pinnedOnly \in BOOLEAN \* Whether to only include pinned detail of a path
    /\ includeProof \in BOOLEAN \* Whether to include cryptographic proof
    /\ (ledger[j][path] # EmptyValue \/ stage[j][path] # EmptyValue) \* value is in either the stage or the ledger
    /\ UNCHANGED <<ledger, stage, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

stage_get(j, path) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ stage[j][path] # EmptyValue
    /\ UNCHANGED <<ledger, stage, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

stage_set(j, path, value) == 
    /\ j \in JournalNames
    /\ path \in Paths
    /\ value \in Values
    /\ stage' = [stage EXCEPT ![j] = [stage[j] EXCEPT ![path] = value]] \* path updated with value in jourmal stage
    /\ UNCHANGED <<ledger, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

ledger_pin(j, path) ==
    /\ j \in JournalNames
    /\ path \in Paths
    /\ committed[j][path] # EmptyValue \* can only pin committed paths
    /\ stage[j][path] = EmptyValue \* path not in stage
    /\ pins' = [pins EXCEPT ![j] = pins[j] \cup {path}] \* add to pinned set
    /\ perm' = [perm EXCEPT ![j] = [perm[j] EXCEPT ![path] = committed[j][path]]]
    /\ UNCHANGED <<ledger, stage, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, tempIndex>>

ledger_unpin(j, path) ==
    /\ j \in JournalNames
    /\ path \in pins[j] \* currently pinned
    /\ pins' = [pins EXCEPT ![j] = pins[j] \ {path}] \* remove pinned
    /\ perm' = [perm EXCEPT ![j] = [perm[j] EXCEPT ![path] = EmptyValue]]
    /\ UNCHANGED <<ledger, stage, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, tempIndex>>



\* register/update bridge connections
bridge(source, target, interface, info) ==
    /\ <<source, target>> \in BridgeIds
    /\ interface \in Values
    /\ info \in Values
    /\ \/ \E mode \in {"push", "pull"}: \* bridge gets some mode, set bridge
        bridges' = [bridges EXCEPT ![<<source, target>>] = [
            @ EXCEPT 
            !.interface = interface,
            !.valid = TRUE,
            !.mode = mode,
            !.pushAllowed = (mode # "pull"),
            !.pullAllowed = (mode # "push")
        ]]
        \/ bridges' = [bridges EXCEPT ![<<source, target>>] = [ \* neither push or pull, bridge cleared
            @ EXCEPT 
            !.valid = FALSE,
            !.pushAllowed = FALSE,
            !.pullAllowed = FALSE
        ]]
    /\ UNCHANGED <<ledger, stage, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>


\* push to local bridge
bridgePush(source, target) ==
    /\ <<source, target>> \in BridgeIds
    /\ bridges[<<source, target>>].valid
    /\ bridges[<<source, target>>].pushAllowed
    /\ \E path \in Paths:
        ledger[source][path] # EmptyValue /\ ledger[source][path] # ledger[target][path] \* must be different, must have some change to push
    /\ stage' = 
        [stage EXCEPT ![target] = [p \in Paths |-> 
            IF ledger[source][p] # EmptyValue /\ ledger[source][p] # ledger[target][p]
            THEN ledger[source][p] \* stage value from source
            ELSE stage[target][p]]] 
    /\ bridges' = [bridges EXCEPT ![<<source, target>>] = [
        @ EXCEPT
        !.lastSyncIndex = stepIndex[source]
        ]]
    /\ UNCHANGED <<ledger, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>


\* pull changes to local ledger 
bridgePull(source, target) ==
    /\ <<source, target>> \in BridgeIds
    /\ bridges[<<source, target>>].valid
    /\ bridges[<<source, target>>].pullAllowed
    /\ \E path \in Paths: \* must have remote changes
        ledger[target][path] # ledger[source][path] /\ ledger[target][path] # EmptyValue
    /\ stage' = [stage EXCEPT ![source] = [p \in Paths |-> \* merge changes, stage changes
        IF ledger[target][p] # EmptyValue /\ \* target has val
           ledger[target][p] # ledger[source][p] \* target is different from source value
        THEN ledger[target][p] \* stage val from remote
        ELSE stage[source][p]]] \* else use local
    /\ bridges' = [bridges EXCEPT ![<<source, target>>] = [ 
        @ EXCEPT \* whatever is in the current bridge stays the same except for the lastSyncIndex and the remoteLedger value
        !.lastSyncIndex = stepIndex[source]
        ]]
    /\ UNCHANGED <<ledger, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

synchronize(id, index) ==
    /\ id \in BridgeIds
    /\ index \in IndexSet
    /\ bridges[id].valid
    /\ \/ (bridges[id].pushAllowed /\ bridgePush(id[1], id[2]))
       \/ (bridges[id].pullAllowed /\ bridgePull(id[1], id[2]))
    /\ UNCHANGED <<pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm, tempIndex>>

Next ==
    \/ \E j \in JournalNames: \E path \in Paths, pinnedOnly \in BOOLEAN, includeProof \in BOOLEAN: resolve(j, path, pinnedOnly, includeProof)
    \/ \E j \in JournalNames: \E path \in Paths: stage_get(j, path)
    \/ \E j \in JournalNames: \E path \in Paths, value \in Values: stage_set(j, path, value)
    \/ \E j \in JournalNames: \E path \in Paths: ledger_pin(j, path)
    \/ \E j \in JournalNames: \E path \in Paths: ledger_unpin(j, path)
    \/ \E id \in BridgeIds: \E interface \in Values, info \in Values: bridge(id[1], id[2], interface, info)
    \/ \E id \in BridgeIds: \E index \in IndexSet: synchronize(id, index)
    \/ time
    \/ \E j \in JournalNames: step(j)
    \/ \E id \in BridgeIds: bridgePush(id[1], id[2])
    \/ \E id \in BridgeIds: bridgePull(id[1], id[2])





\* gives minimum of a function, only used by WindowConstraints
Min(S) == 
    CHOOSE m \in S : \A x \in S : m <= x

\* window of recent states (windowPosition = current position, config.window = how many states to keep)
WindowConstraints ==
    /\ \A j \in JournalNames: \A path \in Paths: (temp[j][path] # EmptyValue) => (windowPosition[j] - tempIndex[j][path] <= config[j].window)
    /\ \A j \in JournalNames: windowPosition[j] >= 0  \* not neg


stepIndexInvariant ==
    /\ \A j \in JournalNames: stepIndex[j] \in Nat \* must be natural
    /\ \A j \in JournalNames: stepIndex[j] <= timeCounter \* less steps than time

BridgeConsistency ==
    /\ \A id \in BridgeIds:
        /\ ~bridges[id].valid => ~bridges[id].pushAllowed /\ ~bridges[id].pullAllowed  \* Invalid bridges cannot push or pull
        /\ bridges[id].valid => (bridges[id].pushAllowed <=> (bridges[id].mode # "pull")) \* don't push unless it is push, etc.
        /\ bridges[id].valid => (bridges[id].pullAllowed <=> (bridges[id].mode # "push"))

PinnedPersistence == \* pinned path is always in temp or perm chain
    \A j \in JournalNames: \A path \in Paths:
        IsPinned(j, path) => (perm[j][path] # EmptyValue)

SafetyInvariant ==
    /\ stepIndexInvariant
    /\ WindowConstraints
    /\ TypeInvariant
    /\ BridgeConsistency
    /\ PinnedPersistence






\* Liveness Properties 

\* Bridges eventually become valid
SynchronizationConvergence ==
    \A id \in BridgeIds: []<> (bridges[id].valid = TRUE)

\*  Committed paths eventually become resolvable, staged value becomes resolvable ledger value
PathAvailability ==
    \A j \in JournalNames: \A path \in Paths: [] ((stage[j][path] # EmptyValue) ~> (ledger[j][path] # EmptyValue))

\* System eventually makes progress
ErrorRecovery ==
    []<> (timeCounter > 0)



\* Fairness operations: Weak fairness

wfstep ==
    \A j \in JournalNames: WF_vars(step(j))

\* Values must eventually be committable
wfset ==
    WF_vars(\E j \in JournalNames, path \in Paths, value \in Values: stage_set(j, path, value))

\* bridges must eventually synchronize
wfsynchronize ==
    WF_vars(\E id \in BridgeIds, index \in IndexSet: synchronize(id, index))

\* paths must eventually resolve
wfresolve ==
    WF_vars(\E j \in JournalNames, path \in Paths: resolve(j, path, FALSE, FALSE))

wftime ==
    WF_vars(time)

wfunpin ==
    WF_vars(\E j \in JournalNames, path \in Paths: ledger_unpin(j, path))

wfbridgePush ==
    \A id \in BridgeIds: WF_vars(bridgePush(id[1], id[2]))

wfbridgePull ==
    \A id \in BridgeIds: WF_vars(bridgePull(id[1], id[2]))



\* Strong Fairness

sfbridgeSynchronize ==
    SF_vars(\E id \in BridgeIds, index \in IndexSet: synchronize(id, index))

Fairness ==
    /\ wfset
    /\ wfsynchronize
    /\ wfresolve
    /\ wftime
    /\ wfunpin
    /\ wfstep
    /\ wfbridgePush
    /\ wfbridgePull
    /\ sfbridgeSynchronize


Spec ==
    Init /\ [][Next]_vars /\ Fairness

THEOREM Spec => []SafetyInvariant

THEOREM Spec => SynchronizationConvergence
THEOREM Spec => PathAvailability
THEOREM Spec => ErrorRecovery



\* any path that is within the window or is pinned (and not subsequently unpinned) should be resolvable 
SingleJournalAvailability ==
    \A j \in JournalNames: \A path \in Paths :
        [](((IsWithinWindow(j, path)) \/ (IsPinned(j, path))) => \* in window or pinned
           (ledger[j][path] # EmptyValue)) \* resolvable, has value


\* any resolvable path that has been committed always returns the same value
SingleJournalImmutability ==
    [] (\A j \in JournalNames: \A path \in Paths: 
        ledger[j][path] # EmptyValue => committed[j][path] = ledger[j][path]) \* committed and ledger must have same value
        
\* any path this is resolvable on a single journal and reachable across bridged journals is also resolvable  
MultiJournalAvailability == 
    \A id \in BridgeIds:
        \A path \in Paths:
            [] ((bridges[id].valid /\ ledger[id[1]][path] # EmptyValue) \* resolvable and bridged
                ~> (ledger[id[2]][path] # EmptyValue)) \* remote journal is resolvable


\* any resolvable path through bridged journals that has been committed always returns the same value
MultiJournalImmutability ==
    \A id \in BridgeIds:
        \A path \in Paths: 
            [] ((bridges[id].valid /\ ledger[id[2]][path] # EmptyValue) => \* bridge is valid, remote journal has value
                [] (ledger[id[2]][path] # EmptyValue)) \* remote and local has same value

THEOREM Spec => SingleJournalAvailability
THEOREM Spec => SingleJournalImmutability
THEOREM Spec => MultiJournalAvailability
THEOREM Spec => MultiJournalImmutability


StepProgression ==
    \A j \in JournalNames: []<> (stepIndex[j] > 0)  \* Steps eventually occur


WindowProgression ==
    \A j \in JournalNames: []<> (windowPosition > 0)  \* Window eventually moves


WindowRetention == \* non empty states in temp chain stay within the retention window
    [] (\A j \in JournalNames: \A path \in Paths:
        (temp[j][path] # EmptyValue) => (windowPosition[j] - tempIndex[j][path] <= config[j].window))




THEOREM Spec => StepProgression
THEOREM Spec => WindowProgression
THEOREM Spec => WindowRetention


====

