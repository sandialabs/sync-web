#!/usr/bin/env python3
"""Exercise a fresh single-node install through real identity, Gateway, and UI assets."""

from __future__ import annotations

import argparse
import json
from typing import Any

import requests


def expect(response: requests.Response, status: int, label: str) -> requests.Response:
    if response.status_code != status:
        raise AssertionError(f"{label}: expected HTTP {status}, got {response.status_code}: {response.text[:2000]}")
    return response


def registration(base: str, username: str, password: str) -> tuple[str, str]:
    flow = expect(
        requests.get(f"{base}/auth/.ory/self-service/registration/api", timeout=10),
        200,
        "registration flow",
    ).json()["id"]
    response = expect(
        requests.post(
            f"{base}/auth/.ory/self-service/registration?flow={flow}",
            json={"method": "password", "password": password, "traits": {"username": username}},
            timeout=10,
        ),
        200,
        "user registration",
    ).json()
    return response["identity"]["id"], response["session_token"]


def login(base: str, username: str, password: str) -> tuple[str, str]:
    flow = expect(
        requests.get(f"{base}/auth/.ory/self-service/login/api", timeout=10),
        200,
        "login flow",
    ).json()["id"]
    response = expect(
        requests.post(
            f"{base}/auth/.ory/self-service/login?flow={flow}",
            json={"method": "password", "identifier": username, "password": password},
            timeout=10,
        ),
        200,
        "login",
    ).json()
    return response["session"]["identity"]["id"], response["session_token"]


def api_token(base: str, session_token: str, description: str) -> str:
    return expect(
        requests.post(
            f"{base}/api/v1/tokens",
            headers={"x-session-token": session_token},
            json={"description": description},
            timeout=10,
        ),
        201,
        "API token creation",
    ).json()["token"]


def post_json(base: str, operation: str, token: str, body: dict[str, Any]) -> requests.Response:
    return requests.post(
        f"{base}/api/v1/general/{operation}",
        headers={"authorization": f"Bearer {token}", "content-type": "application/json"},
        json=body,
        timeout=30,
    )


def post_scheme(base: str, operation: str, token: str, body: str) -> requests.Response:
    return requests.post(
        f"{base}/api/v1/general/{operation}",
        headers={"authorization": f"Bearer {token}", "content-type": "application/scheme"},
        data=body.encode(),
        timeout=30,
    )


def response_value(response: requests.Response) -> Any:
    try:
        return response.json()
    except requests.exceptions.JSONDecodeError:
        return response.text.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="http://127.0.0.1:8192")
    parser.add_argument("--admin-username", default="admin")
    parser.add_argument("--admin-password", default="password")
    parser.add_argument("--username", default="release-qa-user")
    parser.add_argument("--password", default="Tundra!Violet7-Circuit")
    parser.add_argument("--other-username", default="release-qa-other")
    parser.add_argument("--other-password", default="Quartz!Falcon8-Lantern")
    args = parser.parse_args()
    base = args.base.rstrip("/")

    admin_id, admin_session = login(base, args.admin_username, args.admin_password)
    admin_token = api_token(base, admin_session, "release QA admin")
    admins = expect(post_json(base, "admins", admin_token, {}), 200, "admin policy read").json()
    if admins != {"*state*": args.admin_username}:
        raise AssertionError(f"admin policy read returned an unexpected principal: {admins!r}")

    user_id, user_session = registration(base, args.username, args.password)
    user_token = api_token(base, user_session, "release QA user")
    other_id, other_session = registration(base, args.other_username, args.other_password)
    other_token = api_token(base, other_session, "release QA other user")
    program_path = ["*state*", args.username, "release-qa", "program"]
    result_path = ["*state*", args.username, "release-qa", "result"]
    program_path_scheme = "(" + " ".join(program_path) + ")"
    program = (
        f"((path {program_path_scheme}) "
        "(value (lambda (journal path value) "
        "((journal 'set!) path value) ((journal 'get) path))) "
        "(expression? #t))"
    )
    expect(post_scheme(base, "set", user_token, program), 200, "staged program write")

    call_body = {"path": program_path, "arguments": [result_path, "journey-value"]}
    cross_rule = {
        "principal": ["*state*", args.other_username],
        "path": ["release-qa"],
        "get": True,
        "set!": True,
        "resolve": False,
    }
    cross_envelope = {"user": ["*state*", args.username], "rule": cross_rule}
    expect(post_json(base, "get", other_token, {"path": program_path, "expression?": True}), 400, "cross-user get denial")
    cross_write = {
        "path": ["*state*", args.username, "release-qa", "cross-user-write"],
        "value": "forbidden",
        "expression?": True,
    }
    expect(post_json(base, "set", other_token, cross_write), 400, "cross-user write denial")
    expect(post_json(base, "call", other_token, call_body), 400, "cross-user call denial")
    expect(post_json(base, "authorize", other_token, cross_envelope), 400, "cross-user grant denial")
    expect(post_json(base, "call", user_token, call_body), 400, "owner call default denial")

    attempted_call_rule = {
        "principal": ["*state*", args.username],
        "path": ["release-qa"],
        "get": True,
        "set!": False,
        "call!": True,
        "resolve": False,
    }
    attempted_call_envelope = {
        "user": ["*state*", args.username],
        "rule": attempted_call_rule,
    }
    expect(
        post_json(base, "authorize", user_token, attempted_call_envelope),
        400,
        "removed call grant rejection",
    )
    expect(post_json(base, "call", user_token, call_body), 400, "policy cannot grant owner call")
    call_result = response_value(expect(post_json(base, "call", admin_token, call_body), 200, "admin staged call"))
    if call_result != "journey-value":
        raise AssertionError(f"call returned {call_result!r}")
    get_result = response_value(expect(
        post_json(base, "get", user_token, {"path": result_path, "expression?": True}),
        200,
        "call write readback",
    ))
    if get_result != "journey-value":
        raise AssertionError(f"call readback returned {get_result!r}")

    expect(post_json(base, "call", user_token, call_body), 400, "owner remains denied after admin call")
    expect(
        post_json(base, "call", admin_token, {"path": program_path, "arguments": {"arguments": []}}),
        400,
        "nested arguments rejection",
    )
    expect(
        post_json(
            base,
            "call",
            admin_token,
            {"path": program_path, "arguments": [], "$federation": {"route": ["peer"]}},
        ),
        400,
        "federated call rejection",
    )

    help_api = expect(requests.get(f"{base}/workbench/help-api.json", timeout=10), 200, "Workbench help").json()
    if "call!" not in help_api:
        raise AssertionError("Workbench help does not expose call!")
    openapi = expect(requests.get(f"{base}/api/v1/docs/json", timeout=10), 200, "OpenAPI JSON").json()
    if "/api/v1/general/call" not in openapi.get("paths", {}):
        raise AssertionError("OpenAPI does not expose /api/v1/general/call")

    print(json.dumps({"status": "pass", "admin_id": admin_id, "user_id": user_id, "other_id": other_id}, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
