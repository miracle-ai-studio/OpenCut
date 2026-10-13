"""Offline worker proof fence checks; real VF instancing lives in test_derive_static."""
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import run_derived as worker

class WorkerTests(unittest.TestCase):
    def execute(self, expected, actual):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory)
            (base/'source').write_bytes(b'fixed-source')
            (base/'license').write_bytes(b'fixed-notice')
            (base/'request').write_text(json.dumps(dict(source_file=str(base/'source'),license_file=str(base/'license'),proof=expected)))
            with patch.object(worker.resource,'setrlimit'), patch.object(worker.threading,'Thread'), patch.object(worker.derive_static,'derive',return_value=(b'static-output',actual)) as derive:
                worker.run(base/'request',base/'output')
                self.assertEqual(derive.call_args.args[:2],(b'fixed-source',b'fixed-notice'))
                return (base/'output').read_bytes()
    def test_exact_pinned_proof_publishes_only_expected_bytes(self):
        proof=dict(source={'axes':{'wght':400}},tool={'version':'fixed'},output_sha256='fixed')
        self.assertEqual(self.execute(proof,proof),b'static-output')
    def test_numeric_identity_and_changed_output_are_rejected(self):
        proof=dict(source={'axes':{'wght':400}},tool={'version':'fixed'},output_sha256='fixed')
        for changed in (dict(proof,source={'axes':{'wght':400.0}}),dict(proof,output_sha256='changed')):
            with self.assertRaisesRegex(ValueError,'differs from pinned'):
                self.execute(proof,changed)

if __name__=='__main__': unittest.main()
