#!/usr/bin/env python3
"""Continue schema7 acceptance using the retained export/import and source CAS.

No fixture media generation, compilation, transcription or video export. The
original requests live in agent-history-package-v1-checkpoint.json. Structural
comparison infers the revision mapping from the graph and documents, without
using the product's identity hashing implementation as its expected result.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import runpy

from acceptance import Harness


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, ensure_ascii=False,
                                    separators=(",", ":")).encode()).hexdigest()


def database(workspace):
    return sqlite3.connect(f"file:{workspace / 'workspace.sqlite'}?mode=ro", uri=True)


def graph(db, project):
    revisions = db.execute("SELECT revision,parent_revision,transaction_id,document_json,created_at "
                           "FROM revisions WHERE project_id=? ORDER BY revision", [project]).fetchall()
    transactions = db.execute("SELECT transaction_id,base_revision,revision,request_json,result_json,created_at "
                              "FROM editor_transactions WHERE project_id=? ORDER BY transaction_id", [project]).fetchall()
    branches = db.execute("SELECT branch_name,head_revision,created_from_revision,created_at "
                          "FROM project_branches WHERE project_id=? ORDER BY branch_name", [project]).fetchall()
    merges = db.execute("SELECT revision,source_revision FROM project_merge_parents WHERE project_id=? ORDER BY revision", [project]).fetchall()
    head = db.execute("SELECT head_revision FROM projects WHERE project_id=?", [project]).fetchone()[0]
    return dict(revisions=revisions, transactions=transactions, branches=branches, merges=merges, head=head)


def full_graph(source, target, source_id, target_id, package, output):
    old, new = graph(source, source_id), graph(target, target_id)
    assert len(old["revisions"]) == len(new["revisions"]) == package["revision_count"]
    assert len(old["transactions"]) == len(new["transactions"]) == package["transaction_count"]
    old_docs = {r[0]: json.loads(r[3]) for r in old["revisions"]}
    new_docs = {r[0]: json.loads(r[3]) for r in new["revisions"]}
    normalized = {}
    for identity, document in new_docs.items():
        assert document["id"] == target_id
        document = dict(document, id=source_id)
        normalized[identity] = document
    remaining = {r[0]: r for r in new["revisions"]}
    pending = {r[0]: r for r in old["revisions"]}
    revisions, transactions = {}, {}
    while pending:
        progressed = False
        for old_id, row in list(pending.items()):
            if row[1] is not None and row[1] not in revisions:
                continue
            parent = revisions.get(row[1])
            matches = [r for r in remaining.values() if r[1] == parent and r[4] == row[4]
                       and normalized[r[0]] == old_docs[old_id]]
            assert len(matches) == 1, (old_id, len(matches), "Cannot infer an unambiguous graph mapping")
            found = matches[0]
            revisions[old_id] = found[0]
            assert (row[2] is None) == (found[2] is None)
            if row[2] is not None:
                transactions[row[2]] = found[2]
            del remaining[found[0]]
            del pending[old_id]
            progressed = True
        assert progressed, "Revision graph did not converge"
    assert not remaining and revisions[old["head"]] == new["head"]
    assert len(set(revisions.values())) == len(revisions)
    assert len(set(transactions.values())) == len(transactions)
    identities = {**revisions, **transactions, source_id: target_id}

    def translated(value):
        if isinstance(value, str):
            return identities.get(value, value)
        if isinstance(value, list):
            return [translated(v) for v in value]
        if isinstance(value, dict):
            return {k: translated(v) for k, v in value.items()}
        return value

    by_id = {t[0]: t for t in new["transactions"]}
    for original in old["transactions"]:
        actual = by_id[transactions[original[0]]]
        assert actual[1:3] == (revisions[original[1]], revisions[original[2]])
        assert actual[5] == original[5]
        assert json.loads(actual[3]) == translated(json.loads(original[3])), original[0]
        assert json.loads(actual[4]) == translated(json.loads(original[4])), original[0]
    assert new["branches"] == [(name, revisions[head], revisions[base], stamp)
                               for name, head, base, stamp in old["branches"]]
    assert sorted(new["merges"]) == sorted((revisions[a], revisions[b]) for a, b in old["merges"])
    (output / "revision-map.json").write_text(json.dumps(revisions, indent=2) + "\n")
    return {"revisions": len(revisions), "transactions": len(transactions),
            "all_private_documents_equal_except_project_id": True,
            "causal_journals_results_timestamps_and_parent_edges_preserved": True,
            "branches": len(old["branches"]), "merges": len(old["merges"]),
            "source_graph_sha256": digest(old), "restored_graph_sha256": digest(new),
            "revision_map": str(output / "revision-map.json")}


def resources(package_path, workspace):
    manifest = json.loads((package_path / "manifest.json").read_text())
    assert manifest["schema_version"] == 7 and manifest["history_included"]
    total = 0
    for blob in manifest["blobs"]:
        expected = blob["content_hash"]
        archived = package_path / blob["path"]
        restored = workspace / "asset-store" / expected[:2] / expected
        for file in [archived, restored]:
            assert file.is_file() and not file.is_symlink()
            assert file.stat().st_size == blob["size_bytes"]
            assert hashlib.sha256(file.read_bytes()).hexdigest() == expected
        total += blob["size_bytes"]
    return {"complete_blob_count": len(manifest["blobs"]), "bytes": total,
            "independent_package_and_restored_cas_sha256": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--phase", choices=["graph", "faults", "frame", "all"], default="all")
    args = parser.parse_args()
    checkpoint = args.workspace / "agent-history-package-v1-checkpoint.json"
    saved = json.loads(checkpoint.read_text())
    candidate = json.loads(Path(saved["candidate"]).read_text())
    h = Harness(candidate["binary"], args.workspace, "agent-history-package-v1")
    assert h.candidate == candidate["binary_sha256"]
    h.state.setdefault("stages", {})
    target = Path(saved["target_workspace"])
    restored = Harness(candidate["binary"], target, "agent-restored-history-v4")
    package_path = Path(saved["requests"]["import"]["package_path"])
    source_id, target_id = saved["source_project_id"], saved["requests"]["import"]["target_project_id"]
    assert saved["receipts"]["import"] == saved["receipts"]["replay"]

    if args.phase in ("graph", "all"):
        def compare():
            with database(args.workspace) as source, database(target) as destination:
                result = full_graph(source, destination, source_id, target_id,
                                    saved["receipts"]["inspect"], args.report.parent)
                result["resources"] = resources(package_path, target)
                counts = {name: destination.execute("SELECT count(*) FROM " + name).fetchone()[0]
                          for name in ["access_principals", "access_credentials", "access_grants",
                                       "project_plugin_grants", "project_preset_grants", "jobs"]}
                assert not any(counts.values()), counts
                result["authority_and_jobs_not_restored"] = counts
                assert source.execute("SELECT head_revision FROM projects WHERE project_id=?", [source_id]).fetchone()[0] == saved["source"]["revision"]
                return result
        h.stage("independent-graph-and-resource-closure", compare)

    if args.phase in ("faults", "all"):
        def corruption():
            request = dict(saved["requests"]["import"], target_project_id="history-invalid-b1-v1",
                           idempotency_key="history-invalid-b1-v1")
            manifest = json.loads((package_path / "manifest.json").read_text())
            blob = min(manifest["blobs"], key=lambda v: v["size_bytes"])
            file = package_path / blob["path"]
            inode, original = file.stat().st_ino, file.read_bytes()
            assert original and file.stat().st_nlink == 1, "Never mutate shared CAS bytes"
            with database(target) as db:
                before = digest(graph(db, target_id))
                projects = db.execute("SELECT count(*) FROM projects").fetchone()[0]
            try:
                with file.open("r+b") as stream:
                    stream.write(bytes([original[0] ^ 1])); stream.flush(); os.fsync(stream.fileno())
                restored.call("opencut_project_history_package_import", request, error="PACKAGE_INVALID")
            finally:
                with file.open("r+b") as stream:
                    stream.write(original[:1]); stream.flush(); os.fsync(stream.fileno())
            assert file.stat().st_ino == inode and file.read_bytes() == original
            with database(target) as db:
                assert db.execute("SELECT count(*) FROM projects").fetchone()[0] == projects
                assert not db.execute("SELECT 1 FROM projects WHERE project_id=?", [request["target_project_id"]]).fetchone()
                assert digest(graph(db, target_id)) == before
            return {"error": "PACKAGE_INVALID", "no_project_or_history_publication": True,
                    "original_package_bytes_and_inode_restored": True}
        h.stage("corrupted-blob-no-publication", corruption)

        def missing_replay():
            held = package_path.with_name(package_path.name + ".held-for-replay")
            assert package_path.exists() and not held.exists()
            package_path.rename(held)
            try:
                receipt = restored.call("opencut_project_history_package_import", saved["requests"]["import"])
                assert receipt == saved["receipts"]["import"]
            finally:
                held.rename(package_path)
            return {"same_original_receipt_without_package_path": True, "one_original_request": True}
        h.stage("original-replay-before-package-io", missing_replay)
    if args.phase in ("frame", "all"):
        def frame():
            # Render one restored historical revision. Reuse the original external
            # font/Classic reference and unchanged pixel gates, without reimport.
            mapping = json.loads((args.report.parent / "revision-map.json").read_text())
            source_revision = "rev_8ba77cdae8b8d88287744014bb1a77b5b061a0bb456d8a19eb56f77f00f091da"
            original = json.loads((args.workspace / "agent-relocated-runtime-v1-checkpoint.json").read_text())
            request = original["requests"]["one-native-color-png"]["input"]
            request = json.loads(json.dumps(request))
            request["request"].update(project_id=target_id, revision=mapping[source_revision],
                                       idempotency_key=restored.key("historical-color-frame"))
            for key in list(os.environ):
                if key.startswith(("OPENCUT_CLASSIC_", "OPENCUT_SOURCE_ART_", "OPENCUT_FONT_DERIVE_", "OPENCUT_FONT_COLOR_STATIC_")):
                    del os.environ[key]
            os.environ["OPENCUT_ALLOW_PUBLIC_FONT_DOWNLOAD"] = "0"
            before = restored.inspect(target_id)
            job = restored.job("historical-color-frame", "opencut_render_submit", request, timeout=180)
            result = job["extended_result"]["data"]
            assert result["execution"]["host_sha256"] == restored.candidate
            root = Path(saved["candidate"]).parent
            runtime_path = root / "bin/libexec/runtime-bundle.json"
            runtime = json.loads(runtime_path.read_text())["components"]["chromium"]
            browser = root / "bin/libexec" / runtime["entry"]
            assert result["execution"]["browser_version"] == runtime["version"]
            assert result["execution"]["browser_sha256"] == hashlib.sha256(browser.read_bytes()).hexdigest()
            reference = args.report.parent.parent / "color-static-final/grid-classic-reference.rgba"
            previous = json.loads((reference.parent / "journey.json").read_text())["png"]["result"]
            assert result["font_lock"] == previous["font_lock"] and len(result["font_lock"]) == 18
            oracle = runpy.run_path(str(Path(__file__).with_name("verify-relocated-runtime.py")))
            rows = oracle["compare_color_grid_png"](restored.artifact(job), reference)
            assert restored.inspect(target_id) == before
            return {"source_revision": source_revision, "restored_revision": mapping[source_revision],
                    "job_id": job["job_id"], "artifact": job["artifacts"][0], "execution": result["execution"],
                    "font_lock": result["font_lock"], "rows": rows,
                    "reference": str(reference), "reference_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(),
                    "public_download_disabled": True, "current_head_unchanged": True}
        h.stage("restored-fixed-historical-frame", frame)
    args.report.write_text(json.dumps({"candidate": saved["candidate"], "stages": h.state["stages"],
                                      "target_workspace": str(target), "source_project_unchanged": True},
                                     ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
