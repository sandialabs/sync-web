#!/usr/bin/env python3
"""Generate the authority-filtered builtin metadata registry from pinned s7 H_* macros."""
from __future__ import annotations

import argparse
import ast
import hashlib
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT.parent / "journal" / "external" / "s7" / "s7.c"
RUNTIME = ROOT / "src" / "unified_runtime.rs"
OUTPUT = ROOT / "src" / "builtin_metadata.rs"


def rust_raw(text: str) -> str:
    for count in range(1, 12):
        hashes = "#" * count
        if '"' + hashes not in text:
            return f'r{hashes}"{text}"{hashes}'
    raise ValueError("documentation needs an unexpectedly large raw-string delimiter")


def approved_names(runtime: str) -> list[str]:
    names: list[str] = []
    for array in ("APPROVED_ROOT_SYNTAX", "APPROVED_ROOT_BUILTINS"):
        match = re.search(rf"const {array}: &\[&str\] = &\[(.*?)\n\];", runtime, re.S)
        if not match:
            raise ValueError(f"missing {array}")
        names.extend(re.findall(r'"([^"]+)"', match.group(1)))
    return names


def macro_replacements(source: str, prefix: str) -> dict[str, tuple[str, int]]:
    lines = source.splitlines()
    result: dict[str, tuple[str, int]] = {}
    index = 0
    while index < len(lines):
        match = re.match(rf"\s*#define\s+({prefix}_[A-Za-z0-9_]+)\s+(.*)", lines[index])
        if not match:
            index += 1
            continue
        macro, replacement, line = match.group(1), match.group(2), index + 1
        while lines[index].rstrip().endswith("\\") and index + 1 < len(lines):
            replacement = replacement.rstrip()[:-1] + lines[index + 1]
            index += 1
        result[macro] = (replacement.strip(), line)
        index += 1
    return result


def documentation_macros(source: str) -> dict[str, tuple[str, str, int]]:
    lines = source.splitlines()
    docs: dict[str, tuple[str, str, int]] = {}
    index = 0
    while index < len(lines):
        match = re.match(r"\s*#define\s+(H_[A-Za-z0-9_]+)\s+(.*)", lines[index])
        if not match:
            index += 1
            continue
        macro, replacement, line = match.group(1), match.group(2), index + 1
        while lines[index].rstrip().endswith("\\") and index + 1 < len(lines):
            replacement = replacement.rstrip()[:-1] + lines[index + 1]
            index += 1
        tokens = re.findall(r'"(?:\\.|[^"\\])*"', replacement, re.S)
        if tokens:
            try:
                documentation = "".join(ast.literal_eval(token) for token in tokens)
            except (SyntaxError, ValueError):
                index += 1
                continue
            public = re.match(r"\(([^\s()]+)", documentation)
            if public:
                docs.setdefault(public.group(1), (documentation, macro, line))
        index += 1
    return docs



def arity_from_documentation(documentation: str) -> tuple[int, int]:
    depth = 0
    end = 0
    for index, char in enumerate(documentation):
        depth += char == "("
        depth -= char == ")"
        if depth == 0:
            end = index
            break
    form = documentation[1:end]
    parts: list[str] = []
    token = ""
    depth = 0
    for char in form:
        if char.isspace() and depth == 0:
            if token:
                parts.append(token)
                token = ""
            continue
        token += char
        depth += char == "("
        depth -= char == ")"
    if token:
        parts.append(token)
    minimum = maximum = 0
    rest = False
    after_dot = False
    for part in parts[1:]:
        if part in ("...", ".") or part.startswith("."):
            rest = True
            after_dot = True
            continue
        if after_dot:
            continue
        maximum += 1
        if not part.startswith("("):
            minimum += 1
    return minimum, (536_870_912 if rest else maximum)


def signature_atom(identifier: str) -> str | None:
    fixed = {"sc->T": "#t", "sc->F": "#f", "sc->values_symbol": "values", "sc->not_symbol": "not"}
    if identifier in fixed:
        return fixed[identifier]
    match = re.fullmatch(r"sc->is_([A-Za-z0-9_]+)_symbol", identifier)
    if match:
        return match.group(1).replace("_", "-") + "?"
    return None


def parse_signature(expression: str, aliases: dict[str, str]) -> tuple | None:
    expression = aliases.get(expression.strip(), expression.strip())
    tokens = re.findall(r"s7_make_circular_signature|s7_make_signature|sc->[A-Za-z0-9_]+|-?\d+|[(),]", expression)
    position = 0
    def parse():
        nonlocal position
        if position >= len(tokens):
            raise ValueError
        token = tokens[position]
        position += 1
        atom = signature_atom(token)
        if atom is not None:
            return ("atom", atom)
        if token not in ("s7_make_signature", "s7_make_circular_signature"):
            raise ValueError
        if tokens[position] != "(":
            raise ValueError
        position += 1
        if tokens[position] != "sc":  # tokenizer turns sc-> only, plain sc is omitted
            # The tokenizer omits plain sc; tolerate starting comma.
            pass
        # Skip tokens through first comma (the sc argument is not tokenized).
        while position < len(tokens) and tokens[position] != ",":
            position += 1
        position += 1
        first = int(tokens[position]); position += 1
        if tokens[position] != ",": raise ValueError
        position += 1
        if token == "s7_make_circular_signature":
            length = int(tokens[position]); position += 1
            cycle = first
            if tokens[position] != ",": raise ValueError
            position += 1
        else:
            length = first
            cycle = None
        items = []
        for item_index in range(length):
            items.append(parse())
            if item_index + 1 < length:
                if tokens[position] != ",": raise ValueError
                position += 1
        while position < len(tokens) and tokens[position] != ")": position += 1
        if position >= len(tokens): raise ValueError
        position += 1
        return ("circular", cycle, items) if cycle is not None else ("proper", items)
    try:
        result = parse()
        return result
    except (ValueError, IndexError):
        return None


def signature_rust(spec: tuple) -> str:
    if spec[0] == "atom":
        return f"SignatureSpec::Atom({rust_raw(spec[1])})"
    if spec[0] == "proper":
        return "SignatureSpec::Proper(&[" + ", ".join(signature_rust(item) for item in spec[1]) + "])"
    return f"SignatureSpec::Circular {{ cycle_point: {spec[1]}, items: &[" + ", ".join(signature_rust(item) for item in spec[2]) + "] }"

def generate() -> str:
    source_bytes = SOURCE.read_bytes()
    source = source_bytes.decode()
    docs = documentation_macros(source)
    q_macros = macro_replacements(source, "Q")
    aliases: dict[str, str] = {}
    for match in re.finditer(r"sc->([A-Za-z0-9_]+)\s*=\s*(s7_make_(?:circular_)?signature\([^;]+)", source):
        aliases[f"sc->{match.group(1)}"] = match.group(2)
    names = approved_names(RUNTIME.read_text())
    output = [
        "// Generated from the pinned vendored s7 H_* documentation macros.\n",
        "// Regenerate with evaluator/tools/generate-builtin-metadata.py; do not hand-edit entries.\n",
        "pub(crate) const REGISTRY_VERSION: u32 = 1;\n",
        f'pub(crate) const SOURCE_SHA256: &str = "{hashlib.sha256(source_bytes).hexdigest()}";\n',
        'pub(crate) const SOURCE_PATH: &str = "journal/external/s7/s7.c";\n\n',
        "#[derive(Clone, Copy, Debug)]\n",
        "pub(crate) enum SignatureSpec { Atom(&'static str), Proper(&'static [SignatureSpec]), Circular { cycle_point: usize, items: &'static [SignatureSpec] } }\n\n",
        "#[derive(Clone, Copy, Debug)]\n",
        "pub(crate) struct BuiltinMetadata { pub name: &'static str, pub documentation: Option<&'static str>, pub source_macro: Option<&'static str>, pub source_line: Option<u32>, pub minimum_arity: u32, pub maximum_arity: u32, pub signature: Option<SignatureSpec> }\n\n",
        "pub(crate) static ENTRIES: &[BuiltinMetadata] = &[\n",
    ]
    for name in names:
        metadata = docs.get(name)
        if metadata:
            documentation, macro, line = metadata
            minimum, maximum = arity_from_documentation(documentation)
            q_name = "Q_" + macro[2:]
            q_expression = q_macros.get(q_name, ("", 0))[0]
            signature = parse_signature(q_expression, aliases) if q_expression else None
            signature_text = f"Some({signature_rust(signature)})" if signature else "None"
            output.append(
                f"    BuiltinMetadata {{ name: {rust_raw(name)}, documentation: Some({rust_raw(documentation)}), source_macro: Some(\"{macro}\"), source_line: Some({line}), minimum_arity: {minimum}, maximum_arity: {maximum}, signature: {signature_text} }},\n"
            )
        else:
            output.append(
                f"    BuiltinMetadata {{ name: {rust_raw(name)}, documentation: None, source_macro: None, source_line: None, minimum_arity: 0, maximum_arity: 0, signature: None }},\n"
            )
    output.extend([
        "];\n\n",
        "pub(crate) fn metadata(name: &str) -> Option<&'static BuiltinMetadata> { ENTRIES.iter().find(|entry| entry.name == name) }\n",
    ])
    return "".join(output)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    generated = generate()
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != generated:
            print(f"stale generated registry: {OUTPUT}")
            return 1
        print(f"builtin-metadata-check: ok ({len(approved_names(RUNTIME.read_text()))} approved entries)")
        return 0
    OUTPUT.write_text(generated)
    print(f"wrote {OUTPUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
