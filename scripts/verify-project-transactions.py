#!/usr/bin/env python3
"""Real CLI + SQLite fault injection for aggregate project transaction ports."""
import hashlib
import json
import argparse
from pathlib import Path
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, help='Reuse this built CLI without invoking Cargo')
arguments = parser.parse_args()
BIN = arguments.binary.resolve() if arguments.binary else ROOT / 'target/debug/opencut'
RUN = Path(tempfile.mkdtemp(prefix='opencut-project-transactions-')).resolve()
DB = RUN / 'workspace.sqlite'
checks, trace = [], []
print(f'Project transaction acceptance evidence: {RUN}', flush=True)

def invocation(tool, value, label):
    path = RUN / f'{label}.json'
    path.write_text(json.dumps(value))
    return [str(BIN), '--database', str(DB), '--json', 'call', tool, '--input', str(path)]

def read_result(process, label):
    stdout, stderr = process.communicate(timeout=30)
    (RUN / f'{label}.stderr').write_text(stderr)
    response = json.loads(stdout)
    trace.append({'label': label, 'exit': process.returncode, 'response': response})
    return response

def call(tool, value, label, expected=None):
    response = read_result(subprocess.Popen(invocation(tool,value,label), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True),label)
    if expected:
        assert response['error']['code'] == expected, response
    else:
        assert response['error'] is None, response
    return response.get('data')

def sql(statement):
    with sqlite3.connect(DB) as db:
        db.executescript(statement)

def counts():
    with sqlite3.connect(DB) as db:
        return {t: db.execute(f'SELECT COUNT(*) FROM {t}').fetchone()[0]
                for t in ['projects','revisions','editor_transactions','idempotency','outbox']}

def reject(event):
    sql(f"CREATE TRIGGER qa_reject BEFORE INSERT ON outbox WHEN NEW.event_type='{event}' BEGIN SELECT RAISE(ABORT, 'qa injected publication failure'); END;")

def edit(revision, key, name):
    return {'project_id':'p','request':{'expected_revision':revision,'idempotency_key':key,'label':name,
        'operations':[{'type':'sequence.update_settings','sequence_id':'main','name':name}]}}

def snapshot(label):
    return call('opencut_project_inspect',{'project_id':'p'},label)

try:
    if arguments.binary is None:
        subprocess.run(['cargo','build','-p','opencut-cli'],cwd=ROOT,check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    call('opencut_project_list',{},'initialize')
    create={'idempotency_key':'create','project':{'id':'p','name':'Port QA','sequence_id':'main','width':320,'height':180,'fps':{'numerator':24,'denominator':1}}}
    before=counts();reject('project.created')
    call('opencut_project_create',create,'failed-create','STORAGE_ERROR')
    assert counts()==before
    sql('DROP TRIGGER qa_reject;')
    original=call('opencut_project_create',create,'create')
    assert counts()=={'projects':1,'revisions':1,'editor_transactions':0,'idempotency':1,'outbox':1}
    checks.append('creation publication failure rolls back project, revision, receipt and event; exact retry creates once')

    wanted=edit(original['revision'],'edit','Edited')
    before=counts();reject('transaction.committed')
    call('opencut_edit_apply',wanted,'failed-edit','STORAGE_ERROR')
    assert counts()==before and snapshot('after-failed-edit')==original
    sql('DROP TRIGGER qa_reject;')
    committed=call('opencut_edit_apply',wanted,'edit')
    checks.append('edit publication failure rolls back head, revision, transaction, receipt and event; same request succeeds after fault removal')

    restore={'project_id':'p','request':{'expected_revision':committed['revision'],'target_revision':original['revision'],'idempotency_key':'restore','label':'Restore original'}}
    before=counts();reject('transaction.committed')
    call('opencut_revision_restore',restore,'failed-restore','STORAGE_ERROR')
    assert counts()==before and snapshot('after-failed-restore')['revision']==committed['revision']
    sql('DROP TRIGGER qa_reject;')
    restored=call('opencut_revision_restore',restore,'restore')
    assert restored['base_revision']==committed['revision'] and restored['revision']!=original['revision']
    assert snapshot('after-restore')['project']==original['project']
    checks.append('restore publication failure is atomic; retry creates a new revision while preserving immutable history')

    assert call('opencut_project_create',create,'create-replay')==original
    assert call('opencut_edit_apply',wanted,'edit-replay')==committed
    assert call('opencut_revision_restore',restore,'restore-replay')==restored
    checks.append('separate CLI processes replay original create/edit/restore receipts after later head changes')

    before=counts()
    one=edit(restored['revision'],'race-one','One');two=edit(restored['revision'],'race-two','Two')
    processes=[subprocess.Popen(invocation('opencut_edit_apply',v,f'race-{i}'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for i,v in enumerate([one,two])]
    responses=[read_result(p,f'race-{i}') for i,p in enumerate(processes)]
    winners=[r for r in responses if r['error'] is None];losers=[r for r in responses if r['error'] is not None]
    assert len(winners)==len(losers)==1 and losers[0]['error']['code']=='REVISION_CONFLICT',responses
    assert snapshot('after-race')['revision']==winners[0]['data']['revision']
    after=counts();assert {k:after[k]-before[k] for k in before}=={'projects':0,'revisions':1,'editor_transactions':1,'idempotency':1,'outbox':1}
    checks.append('competing CLI processes sharing a base produce one atomic commit and one explicit conflict')

    common=edit(winners[0]['data']['revision'],'same-race','Common')
    before=counts()
    processes=[subprocess.Popen(invocation('opencut_edit_apply',common,f'same-{i}'),stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True) for i in range(4)]
    responses=[read_result(p,f'same-{i}') for i,p in enumerate(processes)]
    assert all(r['error'] is None for r in responses),responses
    assert all(r['data']==responses[0]['data'] for r in responses),responses
    after=counts();assert {k:after[k]-before[k] for k in before}=={'projects':0,'revisions':1,'editor_transactions':1,'idempotency':1,'outbox':1}
    checks.append('four concurrent identical CLI submissions return one transaction and publish one revision/receipt/event')
    report={'status':'passed','checks':checks,'binary_sha256':hashlib.sha256(BIN.read_bytes()).hexdigest(),'counts':counts()}
    (RUN/'report.json').write_text(json.dumps(report,indent=2))
    print(json.dumps({'status':'passed','checks':len(checks),'report':str(RUN/'report.json')},indent=2))
finally:
    (RUN/'trace.json').write_text(json.dumps(trace,indent=2))
