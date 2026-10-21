#!/usr/bin/env python3
"""Hold a nonblocking per-path lock while one uploader worker runs"""

from __future__ import annotations

import fcntl
import hashlib
import os
import subprocess
import sys
from pathlib import Path

BUSY_EXIT_CODE = 75

def main():
    if len(sys.argv) != 4:
        print("Usage: claim_upload.py <lock-directory> <uploader-script> <path>", file=sys.stderr)
        return 2

    lock_directory = Path(sys.argv[1])
    uploader_script = sys.argv[2]
    path = sys.argv[3]
    lock_directory.mkdir(parents=True, exist_ok=True)
    lock_name = hashlib.sha256(os.path.abspath(path).encode()).hexdigest() + ".lock"
    lock_path = lock_directory / lock_name

    with lock_path.open("a+b") as lock_file:
        try:
            fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return BUSY_EXIT_CODE
        return subprocess.run([uploader_script, "--claimed", path], check=False).returncode

if __name__ == "__main__":
    raise SystemExit(main())
