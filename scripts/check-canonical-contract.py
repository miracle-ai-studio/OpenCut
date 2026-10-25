#!/usr/bin/env python3
"""Check the sole final declaration from an explicit integrated CLI;never build."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=os.environ.get("OPENCUT_CONTRACT_BINARY"),
                        help="the already-built integrated CLI (or OPENCUT_CONTRACT_BINARY)")
    args = parser.parse_args()
    if args.binary is None:
        parser.error("provide --binary or OPENCUT_CONTRACT_BINARY;this gate never rebuilds")
    binary = args.binary.resolve(strict=True)
    document = json.loads(subprocess.check_output(
        [str(binary), "system", "schema", "--json"], text=True))
    tools = document.get("data")
    if document.get("error") or not isinstance(tools, list) or not tools:
        raise SystemExit("The integrated CLI did not return a nonempty tool declaration")
    names = [tool["name"] for tool in tools]
    problems = [f"duplicate tool:{name}" for name, count in Counter(names).items() if count != 1]
    for tool in tools:
        name = tool["name"]
        if not re.fullmatch(r"opencut_[a-z][a-z0-9_]*", name):
            problems.append(f"invalid tool name:{name}")
        if re.search(r"(?:^|_)v[0-9]+(?:_|$)", name):
            problems.append(f"development generation in tool:{name}")
        if not isinstance(tool.get("read_only"), bool):
            problems.append(f"missing access classification:{name}")
        for field in ("input_schema", "output_schema"):
            schema = tool.get(field)
            if not isinstance(schema, dict):
                problems.append(f"missing {field}:{name}")
                continue
            # Schema names are current DTOs. Exact dependency/package versions,
            # retained proof bytes and historical document-format fields are data,
            # not alternate business operations and are deliberately not rewritten.
            pending = [schema]
            while pending:
                value = pending.pop()
                if isinstance(value, dict):
                    title = value.get("title")
                    if isinstance(title, str) and re.search(r"V[0-9]+(?:[A-Z_]|$)", title):
                        problems.append(f"development generation in DTO:{name}.{title}")
                    for definitions in (value.get("$defs", {}), value.get("definitions", {})):
                        for definition in definitions:
                            if re.search(r"V[0-9]+(?:[A-Z_]|$)", definition):
                                problems.append(f"development generation in DTO:{name}.{definition}")
                    pending.extend(value.values())
                elif isinstance(value, list):
                    pending.extend(value)
    domain_document = json.loads(subprocess.check_output(
        [str(binary), "system", "domains", "--json"], text=True))
    domains = domain_document.get("data")
    if domain_document.get("error") or not isinstance(domains, list):
        raise SystemExit("The integrated CLI did not return a domain declaration")
    by_name = {tool["name"]: tool for tool in tools}
    owned = set()
    domain_names = set()
    for domain in domains:
        owner = domain.get("domain", "")
        if not re.fullmatch(r"[a-z][a-z0-9_]*", owner) or owner in domain_names:
            problems.append(f"invalid or duplicate domain:{owner}")
        domain_names.add(owner)
        actions = set()
        for entry in domain.get("actions", []):
            action, operation = entry.get("action", ""), entry.get("operation")
            if not re.fullmatch(r"[a-z][a-z0-9]*(?:\.[a-z][a-z0-9]*)*", action) or action in actions:
                problems.append(f"invalid or duplicate action:{owner}.{action}")
            if re.search(r"(?:^|\.)v[0-9]+(?:\.|$)", action):
                problems.append(f"development generation in action:{owner}.{action}")
            actions.add(action)
            if operation not in by_name or operation in owned:
                problems.append(f"missing or duplicate domain ownership:{operation}")
            elif entry.get("read_only") != by_name[operation]["read_only"]:
                problems.append(f"domain access classification differs:{operation}")
            owned.add(operation)
    if owned != set(names):
        problems.append(f"domain declaration omits operations:{sorted(set(names) - owned)}")
    if problems:
        raise SystemExit(json.dumps({"status": "failed", "problems": sorted(set(problems))}, indent=2))
    digest = hashlib.sha256()
    with binary.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    print(json.dumps({"status": "passed", "binary": str(binary), "binary_sha256": digest.hexdigest(),
                      "tools": len(tools), "domains": len(domains),
                      "contract": "one final declaration with exact domain ownership"}, indent=2))


if __name__ == "__main__":
    main()
