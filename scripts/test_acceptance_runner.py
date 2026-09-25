"""Small behavioral checks for durable package acceptance stages."""

from contextlib import redirect_stderr
from contextlib import redirect_stdout
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from acceptance_runner import AcceptanceRunner


class AcceptanceRunnerTest(unittest.TestCase):
    def test_success_has_live_progress_and_durable_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            runner = AcceptanceRunner(directory)
            progress = io.StringIO()
            with redirect_stderr(progress):
                output = runner.run([sys.executable, '-c', 'print("ok")'], name='probe')
            self.assertEqual(output, 'ok\n')
            stages = json.loads((Path(directory) / 'stages.json').read_text())
            self.assertEqual(stages[0]['status'], 'passed')
            self.assertGreaterEqual(stages[0]['elapsed_seconds'], 0)
            self.assertEqual(json.loads(Path(stages[0]['log']).read_text())['stdout'], 'ok\n')
            self.assertIn('START probe', progress.getvalue())
            self.assertIn('PASS probe', progress.getvalue())

    def test_failure_keeps_stage_and_command_output(self):
        with tempfile.TemporaryDirectory() as directory:
            runner = AcceptanceRunner(directory)
            with self.assertRaises(AssertionError):
                runner.run([sys.executable, '-c', 'import sys; print("bad"); sys.exit(3)'],
                           name='failure')
            stage, = json.loads((Path(directory) / 'stages.json').read_text())
            self.assertEqual((stage['status'], stage['exit_code']), ('failed', 3))
            self.assertEqual(json.loads(Path(stage['log']).read_text())['stdout'], 'bad\n')
            runner.flush_commands()
            self.assertEqual(json.loads((Path(directory) / 'commands.json').read_text())[0]['exit_code'], 3)

    def test_timeout_keeps_failed_receipt(self):
        with tempfile.TemporaryDirectory() as directory:
            runner = AcceptanceRunner(directory)
            with self.assertRaises(subprocess.TimeoutExpired):
                runner.run([sys.executable, '-c', 'import time; time.sleep(2)'],
                           timeout=0.02, name='timeout')
            stage, = json.loads((Path(directory) / 'stages.json').read_text())
            self.assertEqual(stage['status'], 'failed')
            self.assertIn('TimeoutExpired', stage['error'])
            self.assertIn('TimeoutExpired', json.loads(Path(stage['log']).read_text())['error'])

    def test_long_stage_reports_monotonic_heartbeat(self):
        with tempfile.TemporaryDirectory() as directory:
            runner = AcceptanceRunner(directory, heartbeat_seconds=0.02)
            progress = io.StringIO()
            with redirect_stderr(progress):
                runner.run([sys.executable, '-c', 'import time; time.sleep(.1)'], name='slow')
            self.assertIn('RUNNING slow', progress.getvalue())

    def test_selected_check_has_its_own_report_and_no_full_receipt(self):
        spec = importlib.util.spec_from_file_location(
            'verify_local_package', Path(__file__).with_name('verify-local-package.py'))
        package = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(package)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / 'bundle'
            (bundle / 'bin').mkdir(parents=True)
            binary = bundle / 'bin/opencut'
            binary.write_bytes(b'delivered binary')
            (bundle / 'manifest.json').write_text(json.dumps({
                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}))
            (bundle / 'doctor.py').write_text('print(\'{"status":"passed"}\')\n')
            archive = root / 'package.tar.gz'
            with tarfile.open(archive, 'w:gz') as tar:
                tar.add(bundle, arcname='bundle')
            Path(str(archive) + '.sha256').write_text(
                hashlib.sha256(archive.read_bytes()).hexdigest() + '  package.tar.gz\n')
            scripts = root / 'scripts'
            scripts.mkdir()
            (scripts / 'check.py').write_text('''import argparse, hashlib, json
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--binary',type=Path);p.add_argument('--artifacts-dir',type=Path);a=p.parse_args()
a.artifacts_dir.mkdir()
r={'status':'passed','checks':['synthetic check'],'binary_sha256':hashlib.sha256(a.binary.read_bytes()).hexdigest()}
path=a.artifacts_dir/'report.json';path.write_text(json.dumps(r))
print(json.dumps({'status':'passed','checks':1,'report':str(path)}))
''')
            evidence = root / 'evidence'
            evidence.mkdir()
            with patch.object(package, 'ROOT', root), \
                 patch.object(package, 'SELECTABLE_CHECKS', {'synthetic': ('check.py', 1, False)}), \
                 patch.object(package.tempfile, 'mkdtemp', return_value=str(evidence)), \
                 redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                package.verify_selected(archive, ['synthetic'])
            report = json.loads((evidence / 'selected-report.json').read_text())
            self.assertEqual(report['mode'], 'selected-checks')
            self.assertEqual(report['results']['synthetic']['checks'], 1)
            self.assertFalse(Path(str(archive) + '.acceptance.json').exists())


if __name__ == '__main__':
    unittest.main()
