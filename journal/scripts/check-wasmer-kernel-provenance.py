#!/usr/bin/env python3
import hashlib
import json
import pathlib
import re
import subprocess
import sys


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


if len(sys.argv) != 3:
    fail(f"usage: {sys.argv[0]} BUNDLE EXPECTED_COMMIT")

bundle = pathlib.Path(sys.argv[1]).resolve()
expected_commit = sys.argv[2]
root = pathlib.Path(__file__).resolve().parents[2]
try:
    actual_commit = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
except (OSError, subprocess.CalledProcessError) as error:
    fail(f"cannot resolve checkout commit: {error}")
if actual_commit != expected_commit:
    fail("checkout commit does not match expected commit")
artifact = bundle / "kernel.wasmer"
wasm = bundle / "kernel.wasm"
manifest = bundle / "source.sha256"
provenance_path = bundle / "provenance.json"
receipts = (
    "builder.txt",
    "cargo.txt",
    "llvm.txt",
    "native-link-libs.sha256",
    "rustc.txt",
    "wasi-clang.txt",
)

for path in (artifact, wasm, manifest, provenance_path):
    if not path.is_file() or path.stat().st_size == 0:
        fail(f"missing provenance bundle file: {path.name}")
for name in receipts:
    path = bundle / name
    if not path.is_file() or path.stat().st_size == 0:
        fail(f"missing toolchain receipt: {name}")

try:
    provenance = json.loads(provenance_path.read_text())
except (OSError, json.JSONDecodeError) as error:
    fail(f"invalid provenance.json: {error}")

sha256 = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
artifact_sha = sha256(artifact)
wasm_sha = sha256(wasm)
manifest_sha = sha256(manifest)
receipt_sha256 = {name: sha256(bundle / name) for name in receipts}

expected = {
    "schema": "sync-web/wasmer-kernel-provenance/v1",
    "source_commit": expected_commit,
    "source_worktree_clean": True,
    "source_manifest_sha256": manifest_sha,
    "wasm_sha256": wasm_sha,
    "artifact_sha256": artifact_sha,
    "runtime": "wasmer-7.2.1-headless",
    "compiler": "wasmer-compiler-llvm-7.2.1",
    "target": "x86_64-unknown-linux-gnu",
    "cpu_features": [],
    "memory_maximum_bytes": 536870912,
    "independent_source_to_wasm_pair": True,
    "independent_source_to_aot_pair": True,
    "receipts_sha256": receipt_sha256,
}
for key, value in expected.items():
    if provenance.get(key) != value:
        fail(f"provenance mismatch for {key}")
if not re.fullmatch(r"[0-9a-f]{64}", artifact_sha):
    fail("artifact SHA-256 is not lowercase hexadecimal")

check = subprocess.run(
    ["sha256sum", "--check", "--strict", str(manifest)],
    cwd=root,
    stdout=subprocess.DEVNULL,
    stderr=subprocess.PIPE,
    text=True,
)
if check.returncode:
    fail(f"source manifest does not match checkout: {check.stderr.strip()}")

print(f"current-source Wasmer provenance verified: {artifact_sha}")
