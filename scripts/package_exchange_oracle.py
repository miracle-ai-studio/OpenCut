"""Independent graph oracle retained from the original history acceptance.

Reads only owned SQLite facts;does not import product validators or identity
mapping code. Media verification callers must hash streams,not read whole files.
"""
import hashlib
import json
import sqlite3

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


