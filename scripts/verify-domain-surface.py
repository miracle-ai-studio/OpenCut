#!/usr/bin/env python3
"""Exercise compact MCP discovery, domain calls, legacy compatibility and authorization."""
import argparse
import json
import os
from pathlib import Path
import selectors
import signal
import socket
import subprocess
import tempfile
import time
from urllib.request import Request, urlopen


def verify(binary):
    with tempfile.TemporaryDirectory(prefix="opencut-domains-") as folder:
        root = Path(folder)
        database = root / "workspace.sqlite"

        def cli(args, payload=None, credential=None, expected=0):
            command = [str(binary), "--database", str(database), "--json"]
            if credential:
                command += ["--credential-file", str(credential)]
            command += args
            result = subprocess.run(
                command,
                input=None if payload is None else json.dumps(payload),
                text=True,
                capture_output=True,
                timeout=30,
            )
            assert result.returncode == expected, (command, result.stdout, result.stderr)
            response = json.loads(result.stdout)
            return response

        catalog = cli(["system", "domains"])["data"]
        assert len(catalog) == 9
        assert sum(len(domain["actions"]) for domain in catalog) == 69
        assert {d["domain"] for d in catalog} == {
            "system", "project", "edit", "media", "task", "observe",
            "plugin", "access", "workspace",
        }
        assert not database.exists(), "static discovery must not create a workspace"

        project = {
            "project": {
                "id": "a", "name": "Domain acceptance", "sequence_id": "main",
                "width": 640, "height": 360,
                "fps": {"numerator": 30, "denominator": 1},
            },
            "idempotency_key": "create-a",
        }
        created = cli(["domain", "project", "create", "--input", "-"], project)["data"]
        assert created["project"]["id"] == "a"
        assert cli(["project", "inspect", "a"])["data"] == created
        assert cli(["domain", "project", "create", "--input", "-"], project)["data"] == created

        policy = {
            "principal_id": "reader", "expected_revision": "0", "enabled": True,
            "grants": [{"project_id": "a", "actions": ["read"]}],
        }
        cli(["access", "set-policy", "--input", "-"], policy)
        secret = root / "reader.token"
        cli(["access", "issue", "reader", "--output", str(secret)])
        assert cli(
            ["domain", "project", "inspect", "--input", "-"],
            {"project_id": "a"}, credential=secret,
        )["data"] == created
        denied = cli(
            ["domain", "workspace", "database.checkpoint", "--input", "-"],
            {"mode": "passive"}, credential=secret, expected=3,
        )
        assert denied["error"]["code"] == "NOT_AUTHORIZED", denied
        assert cli(["project", "inspect", "a"])["data"] == created

        process = subprocess.Popen(
            [str(binary), "--database", str(database), "mcp", "serve"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        pending = bytearray()
        serial = 0

        def request(method, params):
            nonlocal serial
            serial += 1
            process.stdin.write((json.dumps({
                "jsonrpc": "2.0", "id": serial, "method": method, "params": params,
            }) + "\n").encode())
            process.stdin.flush()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if b"\n" not in pending:
                    assert selector.select(max(0, deadline - time.monotonic())), method
                    chunk = os.read(process.stdout.fileno(), 65536)
                    assert chunk, (method, process.poll())
                    pending.extend(chunk)
                while b"\n" in pending:
                    line, _, rest = pending.partition(b"\n")
                    pending[:] = rest
                    message = json.loads(line)
                    if message.get("id") == serial:
                        assert "error" not in message, message
                        return message["result"]
            raise AssertionError(f"MCP timeout: {method}")

        try:
            request("initialize", {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "domain-acceptance", "version": "1"},
            })
            process.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
            process.stdin.flush()
            tools = request("tools/list", {})["tools"]
            assert {tool["name"] for tool in tools} == {
                "opencut_" + domain["domain"] for domain in catalog
            }
            assert all("outputSchema" in tool for tool in tools)
            assert "opencut_project_create" not in {tool["name"] for tool in tools}
            listed = request("resources/list", {})["resources"]
            assert "opencut://schemas/domains" in {resource["uri"] for resource in listed}
            resource = request("resources/read", {"uri": "opencut://schemas/domains/edit"})
            payload = json.loads(resource["contents"][0]["text"])["payload"]["data"]
            assert {action["action"] for action in payload[0]["actions"]} == {"plan", "apply"}
            assert payload[0]["actions"][0]["input_schema"]

            edit = {
                "project_id": "a",
                "request": {
                    "expected_revision": created["revision"],
                    "idempotency_key": "add-track",
                    "label": "Add video track",
                    "operations": [{
                        "type": "track.create", "sequence_id": "main",
                        "track": {
                            "id": "video", "name": "Video", "kind": "video",
                            "enabled": True, "muted": False, "locked": False,
                            "clips": [], "params": {},
                        },
                    }],
                },
            }
            plan = request("tools/call", {
                "name": "opencut_edit", "arguments": {"action": "plan", "input": edit},
            })
            assert not plan.get("isError"), plan
            applied = request("tools/call", {
                "name": "opencut_edit", "arguments": {"action": "apply", "input": edit},
            })
            assert not applied.get("isError"), applied
            revision = applied["structuredContent"]["data"]["revision"]
            legacy = request("tools/call", {
                "name": "opencut_project_inspect", "arguments": {"project_id": "a"},
            })
            assert not legacy.get("isError"), legacy
            assert legacy["structuredContent"]["data"]["revision"] == revision
            assert len(legacy["structuredContent"]["data"]["project"]["sequences"][0]["tracks"]) == 1
        finally:
            process.stdin.close()
            process.wait(timeout=10)
            selector.close()
            assert process.returncode == 0, process.stderr.read().decode()

        with socket.socket() as candidate:
            candidate.bind(("127.0.0.1", 0))
            port = candidate.getsockname()[1]
        token_file = root / "host.token"
        with (root / "host.log").open("wb") as log:
            host = subprocess.Popen([
                str(binary), "--database", str(database), "serve",
                "--listen", f"127.0.0.1:{port}", "--token-file", str(token_file),
            ], stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 15
            while True:
                assert host.poll() is None, (root / "host.log").read_text()
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=.2):
                        break
                except OSError:
                    assert time.monotonic() < deadline, "HTTP host did not start"
                    time.sleep(.03)

            def http(path, payload=None):
                headers = {"Authorization": "Bearer " + token_file.read_text().strip()}
                body = None if payload is None else json.dumps(payload).encode()
                if body is not None:
                    headers["Content-Type"] = "application/json"
                request = Request(
                    f"http://127.0.0.1:{port}{path}", body, headers,
                    method="GET" if body is None else "POST",
                )
                with urlopen(request, timeout=10) as response:
                    return json.load(response)

            http_domains = http("/v1/domains")["data"]
            assert [domain["domain"] for domain in http_domains] == [
                domain["domain"] for domain in catalog
            ]
            edit_contract = http("/v1/domains/edit")["data"]
            assert {action["action"] for action in edit_contract[0]["actions"]} == {"plan", "apply"}
            assert edit_contract[0]["actions"][0]["input_schema"]
            inspected = http("/v1/tools/opencut_project", {
                "action": "inspect", "input": {"project_id": "a"},
            })
            assert inspected["data"]["revision"] == revision
        finally:
            host.send_signal(signal.SIGINT)
            host.wait(timeout=10)
            assert host.returncode == 0, (root / "host.log").read_text()
        return {
            "status": "passed",
            "advertised_mcp_tools": len(tools),
            "mapped_actions": sum(len(domain["actions"]) for domain in catalog),
            "checks": [
                "CLI catalog and idempotent domain project create",
                "restricted caller cannot use owner-only workspace action",
                "MCP compact discovery and exact domain schema resource",
                "MCP domain plan/apply and callable legacy leaf readback",
                "HTTP domain catalog, exact schema and shared project action",
            ],
        }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.binary), ensure_ascii=False, indent=2))
