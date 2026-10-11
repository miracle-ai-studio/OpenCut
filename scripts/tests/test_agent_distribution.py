"""Host config escaping and runtime diagnostics have independent failure oracles."""
import importlib.util
import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import patch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).resolve().parents[1] / 'distribution' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


config = load('agent_config', 'agent-config.py')
doctor = load('doctor', 'doctor.py')


class AgentDistributionTest(unittest.TestCase):
    def test_configs_preserve_spaces_quotes_unicode_and_do_not_open_database(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / '剪辑 "tool" 🎬'
            binary.write_text('fixture'); binary.chmod(0o700)
            database = root / 'new workspace' / '工程.sqlite'
            output = root / 'config'
            report = config.generate(binary, database, output)
            codex = tomllib.loads((output / 'codex.toml').read_text())['mcp_servers']['opencut']
            claude = json.loads((output / 'claude.mcp.json').read_text())['mcpServers']['opencut']
            self.assertEqual(codex['command'], str(binary.resolve()))
            self.assertEqual(codex['args'], claude['args'])
            self.assertIn(str(database.resolve()), claude['args'])
            self.assertIn('--with-worker', codex['args'])
            self.assertTrue(report['managed_worker'])
            self.assertFalse(database.parent.exists())
            with self.assertRaises(FileExistsError):
                config.generate(binary, database, output)
            self.assertEqual(json.loads((output / 'mcp-server.json').read_text()), claude)

    def test_restricted_configuration_references_secret_file_without_reading_secret(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); binary = root / 'opencut'; binary.touch(); binary.chmod(0o700)
            credential = root / 'credential'; credential.write_text('never serialize this secret')
            report = config.generate(binary, root / 'workspace.sqlite', root / 'config', credential=credential)
            self.assertTrue(report['requires_external_worker'])
            for path in (root / 'config').iterdir():
                self.assertNotIn('--with-worker', path.read_text())
                self.assertNotIn('never serialize', path.read_text())

    def test_invalid_names_and_interpolated_paths_rejected_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); binary = root / 'opencut'; binary.touch(); binary.chmod(0o700)
            for name, database in [('bad.name', root / 'db'), ('ok', root / '${OTHER}/db')]:
                with self.assertRaises(ValueError):
                    config.generate(binary, database, root / 'output', name)
                self.assertFalse((root / 'output').exists())

    def test_filter_and_encoder_flags_are_not_assumed_to_have_fixed_width(self):
        names = doctor.registered_names(' T. = Timeline support\n TS geq V->V Generic\n TSC overlay VV->V Overlay\n V....D libx264 Encode\n A....D aac Encode\n')
        self.assertEqual(names, {'geq', 'overlay', 'libx264', 'aac'})

    def test_traversing_manifest_cannot_validate_external_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); outside = root / 'outside'; outside.write_text('fixture')
            bundle = root / 'bundle'; bundle.mkdir()
            (bundle / 'manifest.json').write_text(json.dumps({'target': 'fixture', 'files': {'../outside': {'size': 7, 'sha256': doctor.sha(outside)}}}))
            with patch.object(doctor.shutil, 'which', return_value=None):
                report = doctor.inspect(bundle)
            self.assertEqual(report['status'], 'failed')
            self.assertFalse(report['checks'][0]['passed'])
