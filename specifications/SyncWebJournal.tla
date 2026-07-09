---- MODULE SyncWebJournal ----

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
    MaxIndex

ASSUME
    /\ Values # {}
    /\ MaxWindow \in Nat \ {0}
    /\ Paths # {}
    /\ MaxIndex \in Nat

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


\* StateConstraint: bounds the two unbounded Nat counters so TLC terminates.
StateConstraint == \* safety invariant uses (8,6). liveness uses (12,10)
    /\ timeCounter <= 8
    /\ stepIndex <= 6
    /\ windowPosition <= MaxIndex

Symmetry == Permutations(Paths)


IndexSet == 0..MaxIndex
BridgeNames == {"bridge1", "bridge2"}

EmptyValue == ""
EmptyLedger == [path \in Paths |-> EmptyValue]
IsEmptyLedger(l) == \A p \in Paths: l[p] = EmptyValue

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
vars == <<ledger, stage, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

time ==
    /\ timeCounter' = timeCounter + 1
    /\ UNCHANGED <<ledger, stage, pins, bridges, config, committed, stepIndex, windowPosition, temp, perm>>

IsPinned(path) == path \in pins

TypeInvariant == 
    /\ ledger \in [Paths -> Values] \* Ledger maps paths to values
    /\ stage \in [Paths -> Values]
    /\ \A path \in Paths: stage[path] \in Values  \*all stage values must be in values set
    /\ \A path \in Paths: ledger[path] \in Values
    /\ pins \subseteq Paths \* pins are subset of paths
    /\ bridges \in [BridgeNames -> [
        interface : Values,
        valid : BOOLEAN,
        mode : {"push", "pull"}, 
        lastSyncIndex : Nat, \* Track last time synchronized
        pushAllowed : BOOLEAN, \* push to bridge
        pullAllowed : BOOLEAN,
        remoteLedger : [Paths -> Values] \* doesn't require that the functions are defined for all paths in paths
        ]]  \* for every path, there has to be some defined value. remoteledger function must be defined for all paths
    /\ \A name \in BridgeNames: \A path \in Paths: bridges[name].remoteLedger[path] \in Values \* every path in the remote ledger maps to a valid value
    /\ config \in [window : Nat] \* Config contains window size
    /\ timeCounter \in Nat
    /\ committed \in [Paths -> Values]
    /\ \A path \in Paths: committed[path] \in Values \* All committed values must be in Values set
    /\ stepIndex \in Nat
    /\ windowPosition \in IndexSet  \* Ensure windowPosition stays within bounds
    /\ temp \in [IndexSet -> [Paths -> Values]]   \* Temporary chain stores ledger snapshots, recent states in sliding window
    /\ perm \in [IndexSet -> [Paths -> Values]] \* Permanent chain stores ledger snapshots, immutable history (pin)

Init ==
    /\ ledger = [p \in Paths |-> EmptyValue]  \* Empty ledger initially
    /\ stage = [p \in Paths |-> EmptyValue]
    /\ committed = [p \in Paths |-> EmptyValue]
    /\ pins = {}                             \* No pinned paths initially
     /\ bridges = [b \in BridgeNames |-> [
        interface |-> "",
        valid |-> FALSE,
        mode |-> "pull",
        lastSyncIndex |-> 0,
        pushAllowed |-> FALSE,
        pullAllowed |-> FALSE, \* Invalid bridges cannot allow operations
        remoteLedger |-> [p \in Paths |-> EmptyValue]  \* Initialize with empty values
        ]]
    /\ config = [window |-> MaxWindow]
    /\ timeCounter = 0 
    /\ stepIndex = 0
    /\ windowPosition = 0
    /\ temp = [i \in IndexSet |-> EmptyLedger]
    /\ perm = [i \in IndexSet |-> EmptyLedger]


\* index gets incremented in step, stage gets committed to ledger (committing happens here)
step == 
    /\ windowPosition < MaxIndex
    /\ ledger' = [p \in Paths |-> IF stage[p] # EmptyValue THEN stage[p] ELSE ledger[p]]
    /\ committed' = ledger'
    /\ stage' = [p \in Paths |-> EmptyValue]
    /\ windowPosition' = windowPosition + 1
    /\ temp' = \*
         [index \in IndexSet |->
             IF index = windowPosition
             THEN ledger'
             ELSE IF index <= windowPosition' - config.window \* if index is outside of retention window
                  THEN EmptyLedger
                  ELSE temp[index]]
    /\ perm' = \* gets updated when states age out of the temp window, preserves committed history
         IF windowPosition' >= config.window \* only update when we move past window size
         THEN [perm EXCEPT ![windowPosition' - config.window] = temp[windowPosition' - config.window]] \* copy perm, index gets updated with temp val at index
         ELSE perm
    /\ stepIndex' = stepIndex + 1
    /\ timeCounter' = timeCounter + 1
    /\ UNCHANGED <<pins, bridges, config>>
    
IsWithinWindow(path) == \* simplified, currently checks if path is in sliding window
    \E index \in IndexSet:
        /\ index >= windowPosition - config.window \* index in sliding window (windowPosition - config.window = window lower bound)
        /\ ~IsEmptyLedger(temp[index])  \* checks ledger at index isn't empty



\* ledger and stage operations

resolve(path, pinnedOnly, includeProof) ==
    /\ path \in Paths \* Path must be valid
    /\ pinnedOnly \in BOOLEAN \* Whether to only include pinned detail of a path
    /\ includeProof \in BOOLEAN \* Whether to include cryptographic proof
    /\ (ledger[path] # EmptyValue \/ stage[path] # EmptyValue) \* value is in either the stage or the ledger
    /\ UNCHANGED <<ledger, stage, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

stage_get(path) ==
    /\ path \in Paths
    /\ stage[path] # EmptyValue
    /\ UNCHANGED <<ledger, stage, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

stage_set(path, value) == 
    /\ path \in Paths
    /\ value \in Values
    /\ stage' = [stage EXCEPT ![path] = value] \* path updated with value in stage
    /\ UNCHANGED <<ledger, pins, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

ledger_pin(path) ==
    /\ path \in Paths
    /\ committed[path] # EmptyValue \* can only pin committed paths
    /\ stage[path] = EmptyValue \* path not in stage
    /\ pins' = pins \cup {path} \* add to pinned set
    /\ UNCHANGED <<ledger, stage, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

ledger_unpin(path) ==
    /\ path \in pins \* currently pinned
    /\ pins' = pins \ {path} \* remove pinned
    /\ UNCHANGED <<ledger, stage, bridges, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>



\* register/update bridge connections
bridge(name, interface, info) ==
    /\ name \in BridgeNames
    /\ interface \in Values
    /\ info \in Values
    /\ \/ \E mode \in {"push", "pull"}: \* bridge gets some mode, set bridge
        bridges' = [bridges EXCEPT ![name] = [
            @ EXCEPT 
            !.interface = interface,
            !.valid = TRUE,
            !.mode = mode,
            !.pushAllowed = (mode # "pull"),
            !.pullAllowed = (mode # "push")
        ]]
        \/ bridges' = [bridges EXCEPT ![name] = [ \* neither push or pull, bridge cleared
            @ EXCEPT 
            !.valid = FALSE,
            !.pushAllowed = FALSE,
            !.pullAllowed = FALSE
        ]]
    /\ UNCHANGED <<ledger, stage, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>


\* push to local bridge
bridgePush(name) ==
    /\ name \in BridgeNames
    /\ bridges[name].valid
    /\ bridges[name].pushAllowed
    /\ \E path \in Paths:
        ledger[path] # bridges[name].remoteLedger[path] \* must be different from remote ledger, must have some change to push
    /\ bridges' = [bridges EXCEPT ![name] = [
        @ EXCEPT
        !.lastSyncIndex = stepIndex,
        !.remoteLedger = ledger \* update remote ledger
        ]]
    /\ UNCHANGED <<ledger, stage, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>


\* pull changes to local ledger 
bridgePull(name) ==
    /\ name \in BridgeNames
    /\ bridges[name].valid
    /\ bridges[name].pullAllowed
    /\ \E path \in Paths: \* must have remote changes
        bridges[name].remoteLedger[path] # ledger[path] /\ bridges[name].remoteLedger[path] # EmptyValue
    /\ stage' = [p \in Paths |-> \* merge changes, stage changes
        IF bridges[name].remoteLedger[p] # EmptyValue /\ \* remote ledger has val
           bridges[name].remoteLedger[p] # ledger[p] \* remote is different from local value
        THEN bridges[name].remoteLedger[p] \* stage val from remote
        ELSE stage[p]] \* else use local
    /\ bridges' = [bridges EXCEPT ![name] = [ 
        @ EXCEPT \* whatever is in the current bridge stays the same except for the lastSyncIndex and the remoteLedger value
        !.lastSyncIndex = stepIndex,
        !.remoteLedger = bridges[name].remoteLedger  \* update remote
        ]]
    /\ UNCHANGED <<ledger, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

synchronize(index) ==
    /\ index \in IndexSet
    /\ \E name \in BridgeNames: \* synchronize all valid bridges at index
        /\ bridges[name].valid
        /\ \/ (bridges[name].pushAllowed /\ bridgePush(name)) \* if push allowed, push
           \/ (bridges[name].pullAllowed /\ bridgePull(name))
    /\ UNCHANGED <<stage, pins, config, timeCounter, committed, stepIndex, windowPosition, temp, perm>>

Next ==
    \/ \E path \in Paths, pinnedOnly \in BOOLEAN, includeProof \in BOOLEAN: resolve(path, pinnedOnly, includeProof)
    \/ \E path \in Paths: stage_get(path)
    \/ \E path \in Paths, value \in Values: stage_set(path, value)
    \/ \E path \in Paths: ledger_pin(path)
    \/ \E path \in Paths: ledger_unpin(path)
    \/ \E name \in BridgeNames, interface \in Values, info \in Values: bridge(name, interface, info)
    \/ \E index \in IndexSet: synchronize(index)
    \/ time
    \/ step
    \/ \E name \in BridgeNames: bridgePush(name)
    \/ \E name \in BridgeNames: bridgePull(name)





\* gives minimum of a function, only used by WindowConstraints
Min(S) == 
    CHOOSE m \in S : \A x \in S : m <= x

\* window of recent states (windowPosition = current position, config.window = how many states to keep)
WindowConstraints ==
    /\ (LET NonEmptyTemp == \* temp chain size isn't greater than the window size
        { i \in IndexSet : ~IsEmptyLedger(temp[i]) }  \* find where temp[i] is not empty
        IN
         (NonEmptyTemp = {} \/ ((windowPosition - Min(NonEmptyTemp)) <= config.window))) \* either temp chain is empty, or the oldest non-empty state is within the window
    /\ windowPosition >= 0 \* non neg window position

stepIndexInvariant ==
    /\ stepIndex \in Nat \* must be natural
    /\ stepIndex <= timeCounter \* less steps than time

BridgeConsistency ==
    /\ \A name \in BridgeNames:
        /\ ~bridges[name].valid => ~bridges[name].pushAllowed /\ ~bridges[name].pullAllowed  \* Invalid bridges cannot push or pull
        /\ bridges[name].valid => (bridges[name].pushAllowed <=> (bridges[name].mode # "pull")) \* don't push unless it is push, etc.
        /\ bridges[name].valid => (bridges[name].pullAllowed <=> (bridges[name].mode # "push"))
        /\ \A path \in Paths: bridges[name].remoteLedger[path] \in Values \* all paths in remote ledger must have value


SafetyInvariant ==
    /\ stepIndexInvariant
    /\ WindowConstraints
    /\ TypeInvariant
    /\ BridgeConsistency






\* Liveness Properties 

\* Bridges eventually become valid
SynchronizationConvergence ==
    \A name \in BridgeNames: []<> (bridges[name].valid = TRUE)

\*  Committed paths eventually become resolvable, staged value becomes resolvable ledger value
PathAvailability ==
    \A path \in Paths: [] ((stage[path] # EmptyValue) ~> (ledger[path] # EmptyValue))

\* System eventually makes progress
ErrorRecovery ==
    []<> (timeCounter > 0)



\* Fairness operations: Weak fairness

wfstep ==
    WF_vars(step)

\* Values must eventually be committable
wfset ==
    WF_vars(\E path \in Paths, value \in Values: stage_set(path, value))

\* bridges must eventually synchronize
wfsynchronize ==
    WF_vars(\E index \in IndexSet: synchronize(index))

\* paths must eventually resolve
wfresolve ==
    WF_vars(\E path \in Paths: resolve(path, FALSE, FALSE))

wftime ==
    WF_vars(time)

wfunpin ==
    WF_vars(\E path \in Paths: ledger_unpin(path))

wfbridgePush ==
    WF_vars(\E name \in BridgeNames: bridgePush(name))

wfbridgePull ==
    WF_vars(\E name \in BridgeNames: bridgePull(name))



\* Strong Fairness

sfbridgeSynchronize ==
    SF_vars(\E name \in DOMAIN bridges, index \in IndexSet: synchronize(index))

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
    \A path \in Paths :
        [](((IsWithinWindow(path)) \/ (IsPinned(path))) => \* in window or pinned
           (ledger[path] # EmptyValue)) \* resolvable, has value


\* any resolvable path that has been committed always returns the same value
SingleJournalImmutability ==
    [] (\A path \in Paths: 
        ledger[path] # EmptyValue => committed[path] = ledger[path]) \* committed and ledger must have same value
        
\* any path this is resolvable on a single journal and reachable across bridged journals is also resolvable  
MultiJournalAvailability == 
    \A name \in BridgeNames:
        \A path \in Paths:
            [] ((bridges[name].valid /\ ledger[path] # EmptyValue) \* resolvable and bridged
                ~> (bridges[name].remoteLedger[path] # EmptyValue)) \* remote journal is resolvable


\* any resolvable path through bridged journals that has been committed always returns the same value
MultiJournalImmutability ==
    \A name \in BridgeNames:
        \A path \in Paths: 
            [] ((bridges[name].valid /\ bridges[name].remoteLedger[path] # EmptyValue) => \* bridge is valid, remote journal has value
                [] (bridges[name].remoteLedger[path] = ledger[path])) \* remote and local has same value

THEOREM Spec => SingleJournalAvailability
THEOREM Spec => SingleJournalImmutability
THEOREM Spec => MultiJournalAvailability
THEOREM Spec => MultiJournalImmutability


StepProgression ==
    []<> (stepIndex > 0)  \* Steps eventually occur


WindowProgression ==
    []<> (windowPosition > 0)  \* Window eventually moves


WindowRetention == \* non empty states in temp chain stay within the retention window
    [] (\A index \in IndexSet:
        ~IsEmptyLedger(temp[index]) => (windowPosition - index <= config.window))

PinnedPersistence == \* pinned path is always in temp or perm chain
    \A path \in Paths:
        IsPinned(path) => [] (\E index \in IndexSet: (~IsEmptyLedger(temp[index]) \/ ~IsEmptyLedger(perm[index])))


THEOREM Spec => StepProgression
THEOREM Spec => WindowProgression
THEOREM Spec => WindowRetention
THEOREM Spec => PinnedPersistence


====

