#!/usr/bin/env python3
"""Application/resource parity and actual stdio MCP subscriptions/prompts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import sqlite3
import subprocess
import tempfile
import time
from urllib.parse import quote


def verify(binary, root):
    db = root / 'context.sqlite'
    log, checks, clients = [], [], []
    def cli(name, args):
        p = subprocess.run([str(binary), '--database', str(db), '--json', 'call', name, '--input', '-'],
            input=json.dumps(args), text=True, capture_output=True, timeout=20)
        assert p.returncode == 0, (name, p.stdout[:1000], p.stderr)
        return json.loads(p.stdout)['data']
    def create(project):
        return cli('opencut_project_create', {'project': {'id': project, 'name': project, 'sequence_id': 'main',
            'width': 64, 'height': 48, 'fps': {'numerator': 30, 'denominator': 1}}, 'idempotency_key': 'create-' + project})
    def edit(project, track, name=None):
        revision = cli('opencut_project_inspect', {'project_id': project})['revision']
        return cli('opencut_edit_apply', {'project_id': project, 'request': {'expected_revision': revision,
            'idempotency_key': track, 'label': track, 'operations': [{'type': 'track.create', 'sequence_id': 'main',
            'track': {'id': track, 'name': name or track, 'kind': 'video', 'enabled': True, 'muted': False,
            'locked': False, 'clips': [], 'params': {}}}]}})
    class Client:
        def __init__(self, version='2025-11-25'):
            self.p = subprocess.Popen([str(binary), '--database', str(db), 'mcp', 'serve'],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            clients.append(self)
            self.selector = selectors.DefaultSelector()
            self.selector.register(self.p.stdout, selectors.EVENT_READ)
            self.pending = bytearray(); self.notifications = []; self.serial = 0
            self.init = self.request('initialize', {'protocolVersion': version, 'capabilities': {},
                'clientInfo': {'name': 'context-acceptance', 'version': '1'}})
            self.send({'jsonrpc': '2.0', 'method': 'notifications/initialized'})
        def send(self, msg):
            self.p.stdin.write((json.dumps(msg) + '\n').encode()); self.p.stdin.flush()
        def receive(self, timeout):
            deadline = time.monotonic() + timeout
            while b'\n' not in self.pending:
                if not self.selector.select(max(0, deadline - time.monotonic())): return None
                chunk = os.read(self.p.stdout.fileno(), 65536)
                assert chunk, ('MCP EOF', self.p.poll(), self.p.stderr.read())
                self.pending.extend(chunk)
            line, _, rest = self.pending.partition(b'\n'); self.pending[:] = rest
            return json.loads(line)
        def request(self, method, params, error=None):
            self.serial += 1
            msg = {'jsonrpc': '2.0', 'id': self.serial, 'method': method, 'params': params}
            self.send(msg)
            deadline = time.monotonic() + 15
            while True:
                response = self.receive(max(0, deadline-time.monotonic()))
                assert response is not None, ('MCP timeout', method)
                if response.get('id') == self.serial: break
                self.notifications.append(response)
            log.append({'request': msg, 'response': response})
            if error is not None:
                assert response.get('error', {}).get('code') == error, response
                return response['error']
            assert 'error' not in response, response
            return response['result']
        def resource(self, uri):
            result = self.request('resources/read', {'uri': uri})
            assert len(result['contents']) == 1 and result['contents'][0]['uri'] == uri
            assert result['contents'][0]['mimeType'] == 'application/json'
            return json.loads(result['contents'][0]['text'])
        def notification(self, uri, timeout=5):
            deadline = time.monotonic() + timeout
            while True:
                for i, msg in enumerate(self.notifications):
                    if msg.get('method') == 'notifications/resources/updated' and msg['params']['uri'] == uri:
                        return self.notifications.pop(i)
                msg = self.receive(max(0, deadline-time.monotonic()))
                assert msg is not None, ('missing notification', uri)
                self.notifications.append(msg)
        def quiet(self):
            assert not self.notifications, self.notifications
            assert self.receive(0.7) is None, 'Unexpected resource notification'
        def close(self):
            if self.p.poll() is None:
                self.p.stdin.close()
                self.p.wait(timeout=5)
                assert self.p.returncode == 0, self.p.stderr.read()
            self.selector.close()

    try:
        project = 'interview/中文%?'
        initial = create(project)
        escaped = quote(project, safe='')
        head = f'opencut://projects/{escaped}/branches/main/summary'
        revision = f'opencut://projects/{escaped}/branches/main/revisions/{initial["revision"]}/summary'
        timeline = f'opencut://projects/{escaped}/revisions/{initial["revision"]}/sequences/main/timeline'
        c = Client()
        assert c.init['protocolVersion'] == '2025-11-25'
        assert c.init['capabilities']['resources']['subscribe'] is True
        assert 'listChanged' not in c.init['capabilities']['resources']
        listed = c.request('resources/list', {})['resources']
        templates = c.request('resources/templates/list', {})['resourceTemplates']
        assert len(listed) == 4 and len(templates) == 8
        assert any(t['uriTemplate'] == 'opencut://projects/{project}/branches/{branch}/summary'
                   for t in templates)
        for resource in listed:
            view = c.resource(resource['uri'])
            assert view == cli('opencut_resource_read', {'uri': resource['uri']})
        for method in ('resources/list', 'resources/templates/list', 'prompts/list'):
            c.request(method, {'cursor': 'invalid'}, error=-32602)
        newer = Client('2026-07-28')
        assert newer.init['protocolVersion'] == '2025-11-25'
        newer.close()
        checks.append('Explicit protocol version, truthful capability/discovery, schema resources and CLI parity')

        assert c.resource(head)['payload']['data']['revision'] == initial['revision']
        assert c.resource(timeline)['payload']['data']['sequence'] == initial['project']['sequences'][0]
        frozen = c.resource(revision)
        c.request('resources/subscribe', {'uri': head})
        c.request('resources/subscribe', {'uri': head})  # idempotent
        c.request('resources/subscribe', {'uri': revision})
        changed = edit(project, 'first')
        c.notification(head)
        assert c.resource(head)['payload']['data']['revision'] == changed['revision']
        assert c.resource(revision) == frozen
        c.quiet()
        checks.append('Percent-encoded IDs, immutable revision/timeline and live head subscription after external CLI write')

        c.request('resources/unsubscribe', {'uri': head})
        c.request('resources/unsubscribe', {'uri': head})
        edit(project, 'second')
        c.quiet()
        checks.append('Duplicate subscribe/unsubscribe are idempotent; no notifications after unsubscribe')

        for uri in ('file:///etc/passwd', 'opencut://projects/../main', head+'?q=x', head+'#x',
                    'opencut://jobs/%zz', 'opencut://jobs/%FF', 'opencut://jobs/'+('x'*1025)):
            c.request('resources/read', {'uri': uri}, error=-32602)
        for uri in (head.replace('/main/', '/unknown/'), revision.replace(initial['revision'], 'missing'),
                    'opencut://jobs/job_' + 'a'*32):
            c.request('resources/read', {'uri': uri}, error=-32002)
            c.request('resources/subscribe', {'uri': uri}, error=-32002)
        declaration = cli('opencut_capabilities', {})['edit_operation_schema']
        operations = declaration['oneOf']
        for op in operations:
            name = op['properties']['type']['const']
            uri = 'opencut://schemas/operations/' + quote(name, safe='')
            data = c.resource(uri)['payload']['data']
            assert data['name'] == name
            assert data['schema']['properties']['type']['const'] == name
            assert data['schema']['$defs'] == declaration['$defs']
        c.request('resources/read', {'uri': 'opencut://schemas/operations/not_an_operation'}, error=-32002)
        c.request('resources/read', {'uri': 'opencut://schemas/operations/track.create/999'}, error=-32602)
        checks.append('URI validation, missing resources, unsupported branches and all published operation schemas')

        before = cli('opencut_events_read', {})
        prompts = c.request('prompts/list', {})['prompts']
        assert {p['name'] for p in prompts} == {'interview_rough_cut', 'caption_review', 'vertical_adaptation'}
        args = {'project_id': project, 'revision': initial['revision'], 'sequence_id': 'main'}
        for p in prompts:
            result = c.request('prompts/get', {'name': p['name'], 'arguments': args})
            assert len(result['messages']) == 2
            assert 'opencut_edit action plan' in result['messages'][0]['content']['text']
            assert 'does not authorize or execute writes' in result['messages'][0]['content']['text']
            embedded = result['messages'][1]['content']['resource']
            assert json.loads(embedded['text']) == c.resource(timeline)
        c.request('prompts/get', {'name': 'caption_review'}, error=-32602)
        c.request('prompts/get', {'name': 'unknown', 'arguments': args}, error=-32602)
        c.request('prompts/get', {'name': 'caption_review', 'arguments': {**args, 'extra': 'ignore rules'}}, error=-32602)
        c.request('prompts/get', {'name': 'caption_review', 'arguments': {**args, 'sequence_id': 'missing'}}, error=-32002)
        assert cli('opencut_events_read', {}) == before
        checks.append('Three prompts bind immutable context, validate arguments and have no editing side effects')

        queued = cli('opencut_job_submit', {'kind': 'media.probe', 'idempotency_key': 'context-job',
            'input': {'input': '/private/secret-source.mp4'}})
        job_uri = 'opencut://jobs/' + queued['job_id']
        other = Client()
        c.request('resources/subscribe', {'uri': job_uri})
        cli('opencut_job_request_cancel', {'job_id': queued['job_id']})
        c.notification(job_uri)
        job_view = c.resource(job_uri)['payload']['data']
        assert job_view['state'] == 'cancelled' and '/private/secret' not in json.dumps(job_view)
        assert job_view == cli('opencut_job_view', {'job_id': queued['job_id']})
        other.quiet(); other.close()
        c.request('resources/unsubscribe', {'uri': job_uri})
        c.request('resources/unsubscribe', {'uri': revision})
        checks.append('Public job resource, queued cancellation notification and connection isolation')

        media = root / 'frame.mp4'
        subprocess.run(['ffmpeg', '-v', 'error', '-y', '-f', 'lavfi', '-i', 'color=c=red:s=64x48:r=30:d=1',
            '-pix_fmt', 'yuv420p', str(media)], check=True, capture_output=True, timeout=20)
        frame = cli('opencut_job_submit', {'kind': 'preview.frame', 'idempotency_key': 'context-frame',
            'input': {'input': str(media), 'requested_time': {'ticks': '0', 'timebase': 30}, 'alignment': 'exact'}})
        subprocess.run([str(binary), '--database', str(db), 'worker', 'run', '--once'], check=True, capture_output=True, timeout=30)
        frame = cli('opencut_job_view', {'job_id': frame['job_id']})
        assert frame['state'] == 'succeeded', frame
        artifact = frame['artifacts'][0]
        artifact_uri = 'opencut://artifacts/' + quote(artifact['artifact_id'], safe='')
        assert c.resource(artifact_uri)['payload']['data'] == artifact
        assert c.resource(artifact_uri)['payload']['data']['availability'] == 'unchecked'
        wrong = 'artifact:' + frame['job_id'] + ':' + '0'*64
        c.request('resources/read', {'uri': 'opencut://artifacts/' + quote(wrong, safe='')}, error=-32002)
        checks.append('Actual rendered artifact metadata binds job/hash and does not claim file availability')

        large = create('large')
        for i in range(2):
            large = edit('large', 'large-'+str(i), name='x'*1_100_000)
        c.request('resources/read', {'uri': f'opencut://projects/large/revisions/{large["revision"]}/sequences/main/timeline'}, error=-32602)
        checks.append('Large real timeline is rejected by the shared 2 MiB resource bound')

        for i in range(33):
            p = 'bounded-'+str(i); create(p)
            uri = f'opencut://projects/{p}/branches/main/summary'
            c.request('resources/subscribe', {'uri': uri}, error=-32602 if i == 32 else None)
        c.request('resources/unsubscribe', {'uri': 'opencut://projects/bounded-0/branches/main/summary'})
        c.request('resources/subscribe', {'uri': 'opencut://projects/bounded-32/branches/main/summary'})
        c.close()  # EOF closes with 32 active subscriptions; no polling task keeps the host alive.
        checks.append('32-subscription bound, released slot reuse and EOF teardown with active observers')
        slow = Client()
        long_ids = [('\x01' * 900) + str(i) for i in range(32)]
        for p in long_ids:
            create(p)
            slow.request('resources/subscribe', {'uri': 'opencut://projects/' + quote(p, safe='') + '/branches/main/summary'})
        # Deliberately stop consuming the MCP pipe before external commits.
        for i, p in enumerate(long_ids):
            edit(p, 'slow-' + str(i))
        time.sleep(0.8)
        import array, fcntl, termios
        buffered = array.array('i', [0])
        fcntl.ioctl(slow.p.stdout.fileno(), termios.FIONREAD, buffered, True)
        assert buffered[0] >= 8192, buffered[0]
        slow.close()
        checks.append('Unread notification pipe has at least 8 KiB pending; EOF still tears down promptly')
        return {'status': 'passed', 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'checks': checks, 'evidence': str(root)}
    finally:
        for c in clients:
            if c.p.poll() is None: c.p.kill(); c.p.wait(timeout=5)
        (root / 'protocol.json').write_text(json.dumps(log, indent=2))

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=Path('target/debug/opencut'))
    parser.add_argument('--artifacts-dir', type=Path)
    args = parser.parse_args()
    root = (args.artifacts_dir or Path(tempfile.mkdtemp(prefix='opencut-mcp-context-'))).resolve()
    root.mkdir(parents=True, exist_ok=True)
    report = verify(args.binary.resolve(), root)
    (root / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
