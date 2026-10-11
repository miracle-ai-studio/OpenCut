#!/usr/bin/env python3
"""Check reviewed Cargo boundaries, including inactive/target-specific dependencies.

This complements check-contract-boundaries.py (resolved transitive purity).
It does not infer source-level layering from package dependency declarations.
"""
import argparse
from datetime import date
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
KINDS = {None: "normal", "dev": "dev", "build": "build"}


def violations(metadata, policy, today):
    errors = []
    modules = policy["modules"]
    if policy.get("schema_version") != 1:
        return ["Unsupported architecture policy schema"]
    member_ids = set(metadata["workspace_members"])
    packages = {p["name"]: p for p in metadata["packages"] if p["id"] in member_ids}
    root = Path(metadata["workspace_root"]).resolve()
    if set(modules) != set(packages):
        errors.append(f"Unreviewed modules: {sorted(set(packages) - set(modules))}; "
                      f"stale modules: {sorted(set(modules) - set(packages))}")
    exceptions = policy.get("exceptions", [])
    exception_edges = {}
    actual_edges = set()
    graph = {name: set() for name in packages}
    ids = set()
    for item in exceptions:
        key = (item["from"], item["kind"], item["to"])
        if item["id"] in ids or key in exception_edges:
            errors.append(f"Duplicate exception: {item['id']}")
        ids.add(item["id"])
        exception_edges[key] = item
        if (item["from"] not in modules or item["to"] not in modules
                or item["kind"] not in KINDS.values()):
            errors.append(f"Invalid exception edge: {item['id']}")
        for field in ("owner", "reason", "remove_when"):
            if not item.get(field, "").strip():
                errors.append(f"Exception {item['id']} missing {field}")
        try:
            if date.fromisoformat(item["review_by"]) < today:
                errors.append(f"Expired exception: {item['id']} ({item['review_by']})")
        except (KeyError, ValueError):
            errors.append(f"Invalid exception review date: {item['id']}")
    for name, package in packages.items():
        if name not in modules:
            continue
        rule = modules[name]
        expected_manifest = (root / rule["path"] / "Cargo.toml").resolve()
        if Path(package["manifest_path"]).resolve() != expected_manifest:
            errors.append(f"Module moved without review: {name}")
        for field in ("owner", "responsibility", "layer"):
            if not rule.get(field, "").strip():
                errors.append(f"Module {name} missing {field}")
        actual = {kind: set() for kind in KINDS.values()}
        for dependency in package["dependencies"]:
            # Cargo name is the actual package, not the import alias (rename).
            target = dependency["name"]
            kind = KINDS[dependency.get("kind")]
            actual[kind].add(target)
            edge = (name, kind, target)
            actual_edges.add(edge)
            if target in packages:
                dep_path = dependency.get("path")
                target_path = Path(packages[target]["manifest_path"]).parent.resolve()
                if dep_path is None or Path(dep_path).resolve() != target_path:
                    errors.append(f"Workspace dependency replaced by another source: {edge}")
                if kind != "dev":
                    graph[name].add(target)
            elif dependency.get("path"):
                errors.append(f"Path dependency outside reviewed workspace: {edge}")
            allowed = set(rule["dependencies"][kind])
            if target not in allowed and edge not in exception_edges:
                errors.append(f"Unreviewed dependency: {name} --{kind}--> {target}")
            if target in allowed and edge in exception_edges:
                errors.append(f"Exception also permanently allowed: {edge}")
        for kind, targets in rule["dependencies"].items():
            if kind not in actual:
                errors.append(f"Unknown dependency kind: {name}/{kind}")
                continue
            if len(targets) != len(set(targets)):
                errors.append(f"Duplicate dependency allowance: {name}/{kind}")
            for stale in set(targets) - actual[kind]:
                errors.append(f"Remove unused allowance: {name} --{kind}--> {stale}")
    for edge, item in exception_edges.items():
        if edge not in actual_edges:
            errors.append(f"Remove resolved exception: {item['id']}")
    # Test-only adapter fixtures may form cycles; production/build edges may not.
    visiting, visited = [], set()

    def visit(name):
        if name in visiting:
            errors.append("Production dependency cycle: " + " -> ".join(visiting + [name]))
            return
        if name in visited:
            return
        visiting.append(name)
        for target in sorted(graph[name]):
            visit(target)
        visiting.pop()
        visited.add(name)

    for name in sorted(graph):
        visit(name)
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", type=Path, default=ROOT / "design/architecture/dependency-policy.json")
    args = parser.parse_args()
    try:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
            cwd=ROOT, text=True, stderr=subprocess.PIPE))
        policy = json.loads(args.policy.read_text())
        errors = violations(metadata, policy, date.today())
    except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"Architecture check could not complete: {error}", file=sys.stderr)
        return 1
    print(json.dumps({"status": "failed" if errors else "passed", "errors": errors,
                      "modules": len(policy["modules"]),
                      "temporary_exception_edges": len(policy.get("exceptions", []))}, indent=2))
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(main())
