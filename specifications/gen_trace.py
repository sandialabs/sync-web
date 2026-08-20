#!/usr/bin/env python3

# Events that are an actual operation in MultiJournalSyncWebJournal.tla are only going to be translated.
# Everything else that is not translated will be skipped.

import argparse
import hashlib
import json
import sys
from pathlib import Path


class SExprSyntaxError(Exception):
    pass

class Symbol(str):
    pass

class ByteVector: # #u(1 2 3)
    def __init__(self, values):
        self.values = values

    def __repr__(self):
        return f"#u({' '.join(str(v) for v in self.values)})"

class Event: # parse (event [seq] ...)
    def __init__(self, seq, journal, method, args, status, value):
        self.seq = seq
        self.journal = str(journal)
        self.method = str(method)
        self.args = args
        self.status = str(status)
        self.value = value

    def __repr__(self):
        return f"<Event #{self.seq} {self.journal}.{self.method}{self.args} {self.status}>"



# parses text for (), stings, booleans, ...
def tokenize(text):
    tokens = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c in " \t\r\n":
            i += 1
            continue
        if c == ";":
            while i < n and text[i] != "\n":
                i += 1
            continue
        if c in "()":
            tokens.append(c)
            i += 1
            continue
        if c == '"':
            j = i + 1
            buf = []
            while j < n and text[j] != '"':
                if text[j] == "\\" and j + 1 < n:
                    buf.append(text[j + 1])
                    j += 2
                else:
                    buf.append(text[j])
                    j += 1
            if j >= n:
                raise SExprSyntaxError(f"Unterminated string starting at {i}")
            tokens.append(("string", "".join(buf)))
            i = j + 1
            continue
        if text.startswith("#u(", i):
            tokens.append("#u(")
            i += 3
            continue
        if text.startswith("#t", i) and (i + 2 == n or text[i + 2] in " \t\r\n()"):
            tokens.append(("bool", True))
            i += 2
            continue
        if text.startswith("#f", i) and (i + 2 == n or text[i + 2] in " \t\r\n()"):
            tokens.append(("bool", False))
            i += 2
            continue
        j = i
        while j < n and text[j] not in " \t\r\n()":
            j += 1
        atom = text[i:j]
        tokens.append(("atom", atom))
        i = j
    return tokens

# checks if int, float, symbol 
def parse_atom(atom):
    try:
        return int(atom)
    except ValueError:
        pass
    try:
        return float(atom)
    except ValueError:
        pass
    return Symbol(atom)

# takes values into structures 
def parse_tokens(tokens):
    pos = 0

    def parse_expr():
        nonlocal pos
        if pos >= len(tokens):
            raise SExprSyntaxError("Unexpected end of input")
        tok = tokens[pos]
        if tok == "(":
            pos += 1
            items = []
            while pos < len(tokens) and tokens[pos] != ")":
                items.append(parse_expr())
            if pos >= len(tokens):
                raise SExprSyntaxError("Unterminated list")
            pos += 1
            return items
        if tok == "#u(":
            pos += 1
            items = []
            while pos < len(tokens) and tokens[pos] != ")":
                items.append(parse_expr())
            if pos >= len(tokens):
                raise SExprSyntaxError("Unterminated byte-vector")
            pos += 1
            return ByteVector(items)
        if tok == ")":
            raise SExprSyntaxError("Unexpected ')'")
        if isinstance(tok, tuple):
            kind, val = tok
            pos += 1
            if kind == "string":
                return val
            if kind == "bool":
                return val
            if kind == "atom":
                return parse_atom(val)
        raise SExprSyntaxError(f"Unrecognized token {tok!r}")

    exprs = []
    while pos < len(tokens):
        exprs.append(parse_expr())
    return exprs


def parse_sexpr_line(line):
    tokens = tokenize(line)
    exprs = parse_tokens(tokens)
    if len(exprs) != 1:
        raise SExprSyntaxError(f"Expected exactly one top-level form, got {len(exprs)}")
    return exprs[0]





def load_events(log_path): # reads trace.log
    events = []
    errors = []


    # top level parenthesis depth, inside a string, and escape chars 
    def iter_forms(lines):
        buf = []
        start_lineno = None
        paren_depth = 0
        in_string = False
        escape = False

        for lineno, raw_line in enumerate(lines, start=1):
            line = raw_line.rstrip("\n")
            if start_lineno is None and line.strip():
                start_lineno = lineno

            if start_lineno is not None:
                buf.append(line)

                i = 0
                while i < len(line):
                    c = line[i]

                    if in_string:
                        if escape:
                            escape = False
                        elif c == "\\":
                            escape = True
                        elif c == '"':
                            in_string = False
                        i += 1
                        continue

                    if c == '"':
                        in_string = True
                        i += 1
                        continue

                    if c == ";":
                        break  

                    if c == "(":
                        paren_depth += 1
                    elif c == ")":
                        paren_depth -= 1

                    i += 1

                if paren_depth == 0 and not in_string:
                    form_text = "\n".join(buf).strip()
                    if form_text:
                        yield start_lineno, form_text
                    buf = []
                    start_lineno = None

        if start_lineno is not None:
            yield start_lineno, "\n".join(buf).strip()

    with open(log_path) as f: # collects list of events
        for lineno, form in iter_forms(f):
            try:
                parsed = parse_sexpr_line(form)
            except SExprSyntaxError as e:
                preview = form.replace("\n", " ")[:120]
                errors.append(f"line {lineno}: {e} ({preview})")
                continue
            if not isinstance(parsed, list) or len(parsed) != 7 or parsed[0] != "event":
                preview = form.replace("\n", " ")[:120]
                errors.append(f"line {lineno}: not a well-formed (event ...) record: {preview}")
                continue
            _, seq, journal, method, args, status, value = parsed
            events.append(Event(seq, journal, method, args, status, value))

    if errors:
        print("Warning: some lines could not be parsed and were skipped:", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
    return events


# objects -> readable strings
def render(x):
    if isinstance(x, Symbol):
        return str(x)
    if isinstance(x, str):
        return x
    if isinstance(x, bool):
        return "true" if x else "false"
    if isinstance(x, (int, float)):
        return str(x)
    if isinstance(x, ByteVector):
        return "bytes:" + "".join(f"{v:02x}" for v in x.values)
    if isinstance(x, list):
        return "(" + " ".join(render(e) for e in x) + ")"
    return str(x)


def short_hash(text): # path names too long, hashes
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:10]


class SymbolTable: # renames configs
    def __init__(self):
        self.path_map = {}
        self.value_map = {}
        self.journal_map = {}

    def canon_journal(self, sym):
        name = str(sym)
        self.journal_map[name] = name
        return name

    def canon_path(self, path_expr):
        r = render(path_expr)
        if r in self.path_map:
            return self.path_map[r]
        if len(r) <= 60:
            atom = r.strip("()").replace(" ", "/") or "root"
        else:
            atom = "path_" + short_hash(r)
        base, i = atom, 2
        existing = set(self.path_map.values())
        while atom in existing:
            atom = f"{base}#{i}"
            i += 1
        self.path_map[r] = atom
        return atom

    def canon_value(self, value_expr):
        r = render(value_expr)
        if r == "":
            return ""
        if r in self.value_map:
            return self.value_map[r]
        if len(r) <= 40 and "\n" not in r:
            atom = r
        else:
            atom = "val_" + short_hash(r)
        base, i = atom, 2
        existing = set(self.value_map.values())
        while atom == "" or atom in existing:
            atom = f"{base}#{i}"
            i += 1
        self.value_map[r] = atom
        return atom


# in trace log, but not translated 
SKIP_REASONS = {
    "*init*": "constructor, not modeled",
    "config": "no changes to state machine",
    "info": "no changes to state machine",
    "size": "no changes to state machine",
    "~field!": "no changes to state machine",
    "~path-normalize": "no changes to state machine",
    "update-config!": "administrative operation",
    "update-code!": "administrative operation",
    "bridge-head": "multi-hop proof-chain mechanics, not modeled",
    "merge-head": "multi-hop proof-chain mechanics, not modeled",
    "trace": "multi-hop proof-chain mechanics, not modeled",
    "synchronize": "bridgePull/bridgePush already reads the peer's real ledger directly; no action needed",
}

STATE_MARKER = Symbol("*state*")


# takes path and creates segments
# idx default is -1 (current index)
def split_path_index(path_expr, allow_index):
    if not isinstance(path_expr, list) or len(path_expr) == 0:
        return False, None, "empty or malformed path"

    segs = path_expr
    idx = -1

    if allow_index and isinstance(segs[0], int):
        idx = segs[0]
        segs = segs[1:]
    elif not allow_index and isinstance(segs[0], int):
        return False, None, f"unexpected explicit index {segs[0]} on stage path"

    if len(segs) == 0 or segs[0] != STATE_MARKER:
        marker = segs[0] if segs else None
        return False, None, f"non-*state* namespace path (starts with {marker!r}), not modeled"

    return True, idx, segs


def normalize_path(path_expr, allow_index):
    ok, idx, result = split_path_index(path_expr, allow_index)
    if not ok:
        return False, result
    segs = result
    return True, (idx, segs)



# (content __), takes value
def extract_content(value):
    if (isinstance(value, list) and len(value) >= 1 and
            isinstance(value[0], list) and len(value[0]) == 2 and
            value[0][0] == Symbol("content")):
        return value[0][1]
    return value


# for unknown and nothing
def is_sentinel(value):
    return (isinstance(value, list) and len(value) == 1 and
            isinstance(value[0], Symbol) and str(value[0]) in ("unknown", "nothing"))


def as_bool(x):
    return bool(x) if isinstance(x, bool) else (str(x) != "#f")




# briefly tracks to see if bridge transfers would change a state value
# tracks committed ledger and staged values
class SimState:
    def __init__(self):
        self.ledger = {}
        self.stage = {} 

    def get_ledger(self, j, path):
        return self.ledger.get((j, path), "")

    def apply_stage_set(self, j, path, value): # record stage
        self.stage[(j, path)] = value

    def apply_step(self, j): # commits all staged values into ledger
        for (jj, path), value in list(self.stage.items()):
            if jj == j:
                if value != "":
                    self.ledger[(jj, path)] = value
                del self.stage[(jj, path)]

    def transfer_paths(self, from_j, to_j):
        return [
            path for (jj, path), value in self.ledger.items()
            if jj == from_j and value != "" and self.get_ledger(to_j, path) != value # state paths where source and target are not the same
        ]

    def apply_transfer(self, from_j, to_j, paths):
        for path in paths:
            self.stage[(to_j, path)] = self.ledger[(from_j, path)]




# translates events to tla+ modeled operation
# events, journals, paths, and names ----> tla and skipped operations 
def convert(events, symtab, traced_journals):
    ops = []
    skipped = []
    sim = SimState()

    for ev in events:
        if ev.status != "ok":
            skipped.append((ev, "intentional negative in test case")) # error event, skip
            continue

        m = ev.method
        a = ev.args
        j = symtab.canon_journal(ev.journal)

        if m in SKIP_REASONS:
            skipped.append((ev, SKIP_REASONS[m]))
            continue

        if m == "set!": # -> stage_set
            ok, norm = normalize_path(a[0], allow_index=False)
            if not ok:
                skipped.append((ev, norm))
                continue
            _, segs = norm
            raw_value = a[1]
            value = "" if is_sentinel(raw_value) else raw_value
            path_atom = symtab.canon_path(segs)
            value_atom = symtab.canon_value(value)
            ops.append({"op": "stage_set", "j": j, "path": path_atom, "value": value_atom})
            sim.apply_stage_set(j, path_atom, value_atom)

        elif m == "set-batch!": # makes multiple stage_set operations
            paths, values = a[0], a[1]
            for p, v in zip(paths, values):
                ok, norm = normalize_path(p, allow_index=False)
                if not ok:
                    skipped.append((ev, f"{norm} (path {render(p)} within set-batch!)"))
                    continue
                _, segs = norm
                value = "" if is_sentinel(v) else v
                path_atom = symtab.canon_path(segs)
                value_atom = symtab.canon_value(value)
                ops.append({"op": "stage_set", "j": j, "path": path_atom, "value": value_atom})
                sim.apply_stage_set(j, path_atom, value_atom)

        elif m == "get": # -> stage_get
            ok, norm = normalize_path(a[0], allow_index=False)
            if not ok:
                skipped.append((ev, norm))
                continue
            _, segs = norm
            content = extract_content(ev.value)
            if is_sentinel(content):
                skipped.append((ev, "read found no genuine staged value"))
                continue
            if isinstance(content, list) and len(content) > 0 and content[0] == "directory":
                skipped.append((ev, "directory read not modeled"))
                continue
            ops.append({"op": "stage_get", "j": j, "path": symtab.canon_path(segs)})

        elif m == "resolve": # resolve
            ok, norm = normalize_path(a[0], allow_index=True)
            if not ok:
                skipped.append((ev, norm))
                continue
            idx, segs = norm
            pinned_ = as_bool(a[1]) if len(a) > 1 else False
            proof_ = as_bool(a[2]) if len(a) > 2 else False
            head = a[3] if len(a) > 3 else False
            if head not in (False, None) and head != Symbol("#f"):
                skipped.append((ev, "head argument is not modeled"))
                continue
            content = extract_content(ev.value)
            if is_sentinel(content):
                skipped.append((ev, "resolve found no real value"))
                continue
            if isinstance(content, list) and len(content) > 0 and content[0] == "directory":
                skipped.append((ev, "directory resolve not modeled"))
                continue
            ops.append({
                "op": "resolve",
                "j": j,
                "idx": str(idx),
                "path": symtab.canon_path(segs),
                "pinnedOnly": pinned_,
                "includeProof": proof_
            })

        elif m == "pin!": # ledger_pin
            ok, norm = normalize_path(a[0], allow_index=True)
            if not ok:
                skipped.append((ev, norm))
                continue
            idx, segs = norm
            response = a[1] if len(a) > 1 else False
            if response not in (False, None) and response != Symbol("#f"):
                skipped.append((ev, "cross bridge pin is not modeled"))
                continue
            ops.append({
                "op": "ledger_pin",
                "j": j,
                "idx": str(idx),
                "path": symtab.canon_path(segs)
            })

        elif m == "unpin!": # ledger_unpin
            ok, norm = normalize_path(a[0], allow_index=True)
            if not ok:
                skipped.append((ev, norm))
                continue
            idx, segs = norm
            ops.append({
                "op": "ledger_unpin",
                "j": j,
                "idx": str(idx),
                "path": symtab.canon_path(segs)
            })

        elif m == "step!": # step
            ops.append({"op": "step", "j": j})
            sim.apply_step(j)

        elif m == "bridge!": # makes bridge between journals 
            name = str(a[0])
            if name not in traced_journals: # peer journal needs to appear in the traces
                skipped.append((ev, f"bridge peer '{name}' was not traced"))
                continue
            info = a[1] if len(a) > 1 else None
            bridge_op = {
                "op": "bridge",
                "source": j,
                "target": name,
                "interface": symtab.canon_value(info),
                "info": symtab.canon_value(info),
                "mode": None,
            }
            if ev.value is False:
                bridge_op["mode"] = "fail"
            ops.append(bridge_op)

        elif m == "bridge-synchronize!": 
            # bridgePush if data moves from local to peer
            # bridgePull if data moves from peer to local
            # bridgePushNoOp / bridgePullNoOp if nothing changes
            name = str(a[0])
            if name not in traced_journals:
                skipped.append((ev, f"bridge peer '{name}' was not traced"))
                continue

            response = a[2] if len(a) > 2 else False
            if response in (False, None) or response == Symbol("#f"):
                skipped.append((ev, "bridge-synchronize! did not change the state"))
                continue

            if ev.value is False:
                skipped.append((ev, "bridge-synchronize! returned [nothing new / stale / error response], did not transfer data"))
                continue

            direction = a[3] if len(a) > 3 else Symbol("pull")
            direction = str(direction)

            if direction == "push":
                transfer_paths = sim.transfer_paths(j, name)
                if not transfer_paths:
                    ops.append({"op": "bridgePushNoOp", "source": j, "target": name})
                    continue
                ops.append({"op": "bridgePush", "source": j, "target": name})
                sim.apply_transfer(j, name, transfer_paths)
            else:
                transfer_paths = sim.transfer_paths(name, j)
                if not transfer_paths:
                    ops.append({"op": "bridgePullNoOp", "source": j, "target": name})
                    continue
                ops.append({"op": "bridgePull", "source": j, "target": name})
                sim.apply_transfer(name, j, transfer_paths)

        else:
            skipped.append((ev, f"unrecognized method '{m}'"))

    return ops, skipped




# checks later operation of the bridge to determine mode; default is pull
def resolve_bridge_modes(ops):
    for i, op in enumerate(ops):
        if op.get("op") != "bridge" or op.get("mode") is not None:
            continue
        mode = "pull"
        for later in ops[i + 1:]:
            if later.get("source") == op["source"] and later.get("target") == op["target"]:
                if later["op"] in ("bridgePush", "bridgePushNoOp"):
                    mode = "push"
                    break
                if later["op"] in ("bridgePull", "bridgePullNoOp"):
                    mode = "pull"
                    break
        op["mode"] = mode
    return ops


def tla_string(s):
    return '"' + str(s).replace("\\", "\\\\").replace('"', '\\"') + '"'


def tla_bool(b):
    return "TRUE" if b else "FALSE"


# makes records for trace ops
def render_record(op):
    fields = []
    for k, v in op.items():
        if isinstance(v, bool):
            fields.append(f"{k} |-> {tla_bool(v)}")
        elif isinstance(v, int):
            fields.append(f"{k} |-> {v}")
        else:
            fields.append(f"{k} |-> {tla_string(v)}")
    return "[" + ", ".join(fields) + "]"


# LedgerTraceOps.tla
def generate_trace_ops(ops):
    lines = [
        "---- MODULE LedgerTraceOps ----",
        "",
        "EXTENDS Sequences",
        "",
        "TraceEvents == <<",
    ]
    for i, op in enumerate(ops):
        comma = "," if i < len(ops) - 1 else ""
        lines.append(f"    {render_record(op)}{comma}")
    lines.append(">>")
    lines.append("")
    lines.append("====")
    return "\n".join(lines)


# LedgerTrace.cfg
def generate_cfg(journal_names, paths, values, max_window, max_index):
    def tla_set(items):
        return "{" + ", ".join(tla_string(x) for x in sorted(items)) + "}"

    lines = [
        "\\* java -cp tla2tools.jar tlc2.TLC -config LedgerTrace.cfg LedgerTrace.tla",
        "",
        "SPECIFICATION TraceSpec",
        "",
        "CONSTANTS",
        f"    JournalNames = {tla_set(journal_names)}",
        f"    Paths = {tla_set(paths) if paths else tla_set(['_no_paths_seen_'])}",
        f"    Values = {tla_set(list(values) + [''])}",
        f"    MaxWindow = {max_window}",
        f"    MaxIndex = {max_index}",
        "",
        "INVARIANTS",
        "    TypeInvariant",
        "    SafetyInvariant",
        "",
        "PROPERTIES",
        "   SingleJournalAvailability",
        "   SingleJournalImmutability",
        "   MultiJournalAvailability",
        "   MultiJournalImmutability",
    ]
    return "\n".join(lines)





# read trace.log, parse events, convert modeled events into TLA+ operations, 
# create bridge modes, then create: LedgerTraceOps.tla, LedgerTrace.cfg, trace-symbols.json 
# skip other events
def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("trace_log", type=Path)
    parser.add_argument("output_dir", type=Path)
    parser.add_argument("--max-window", type=int, default=4,
                        help="Ledger config's window (test-ledger.scm uses 4)")
    parser.add_argument("--max-index", type=int, default=None,
                        help="Defaults to [max steps seen on any journal] + 4")
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)

    events = load_events(args.trace_log)
    if not events:
        print("No events parsed from trace logs.", file=sys.stderr)
        sys.exit(1)

    traced_journals = {ev.journal for ev in events}
    symtab = SymbolTable()
    ops, skipped = convert(events, symtab, traced_journals)
    ops = resolve_bridge_modes(ops)

    if not ops:
        print("No events mapped to TLA+ specs", file=sys.stderr)

    # sets for config
    journal_names = {op[k] for op in ops for k in ("j", "source", "target") if k in op}
    paths = {op["path"] for op in ops if "path" in op}
    values = {op["value"] for op in ops if "value" in op and op["value"] != ""}
    values |= {op["interface"] for op in ops if "interface" in op and op["interface"] != ""}
    values |= {op["info"] for op in ops if "info" in op and op["info"] != ""}


    # maxIndex calculation (# of step operations + 4)
    step_counts = {}
    for op in ops:
        if op["op"] == "step":
            step_counts[op["j"]] = step_counts.get(op["j"], 0) + 1
    max_steps = max(step_counts.values()) if step_counts else 1
    max_index = args.max_index if args.max_index is not None else max_steps + 4

    (args.output_dir / "LedgerTraceOps.tla").write_text(generate_trace_ops(ops))
    (args.output_dir / "LedgerTrace.cfg").write_text(
        generate_cfg(journal_names, paths, values, args.max_window, max_index)
    )
    (args.output_dir / "trace-symbols.json").write_text(json.dumps({
        "paths": {v: k for k, v in symtab.path_map.items()},
        "values": {v: k for k, v in symtab.value_map.items()},
    }, indent=2))



    print(f"Generated {len(ops)} TLA+ operations from {len(events)} parsed trace events.", file=sys.stderr)
    print(f"Journals: {sorted(journal_names)}", file=sys.stderr)
    print(f"Wrote {args.output_dir / 'LedgerTraceOps.tla'}", file=sys.stderr)
    print(f"Wrote {args.output_dir / 'LedgerTrace.cfg'}", file=sys.stderr)
    print(f"Wrote {args.output_dir / 'trace-symbols.json'}", file=sys.stderr)




    # skipped events
    if skipped:
        print(f"\n{len(skipped)} events skipped:", file=sys.stderr)
        reason_counts = {}
        for ev, reason in skipped:
            reason_counts.setdefault(reason, []).append(ev)
        for reason, evs in reason_counts.items():
            print(f"  [{len(evs)}x] {reason}", file=sys.stderr)
            for ev in evs[:3]:
                print(f"        e.g. #{ev.seq} {ev.journal}.{ev.method}", file=sys.stderr)
            if len(evs) > 3:
                print(f"        ... and {len(evs) - 3} more", file=sys.stderr)


if __name__ == "__main__":
    main()