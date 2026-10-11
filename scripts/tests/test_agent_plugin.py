"""Package boundary checks: no silent target substitution or configuration mutation."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('agent_plugin', Path(__file__).resolve().parents[1] / 'distribution/agent-plugin.py')
PLUGIN = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLUGIN)


class AgentPluginTests(unittest.TestCase):
    def test_exact_connection_and_separate_adapters_without_database_side_effects(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / '剪辑 "tool" 🎬'; binary.write_text('fixture'); binary.chmod(0o700)
            database = root / 'uncreated' / 'film.sqlite'
            output = root / 'plugins'
            receipt = PLUGIN.generate(binary, database, output)
            self.assertFalse(database.parent.exists())
            servers = []
            skills = []
            for client in ('codex', 'claude'):
                plugin = output / client / 'plugins/opencut'
                self.assertTrue((plugin / f'.{client}-plugin/plugin.json').is_file())
                servers.append(json.loads((plugin / '.mcp.json').read_text())['mcpServers']['opencut'])
                skills.append((plugin / 'skills/edit/SKILL.md').read_text())
            self.assertEqual(servers[0], servers[1])
            self.assertEqual(skills[0], skills[1])
            self.assertEqual(servers[0]['args'], ['--database', str(database.resolve()), 'mcp', 'serve', '--with-worker'])
            self.assertEqual(servers[0]['command'], str(binary.resolve()))
            for relative, digest in receipt['files'].items():
                self.assertEqual(hashlib.sha256((output / relative).read_bytes()).hexdigest(), digest)
            before = (output / 'manifest.json').read_bytes()
            with self.assertRaises(FileExistsError):
                PLUGIN.generate(binary, database, output)
            self.assertEqual((output / 'manifest.json').read_bytes(), before)

    def test_existing_credential_is_referenced_never_copied_or_expanded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'opencut'; binary.touch(); binary.chmod(0o700)
            credential = root / 'credential'; credential.write_text('SECRET FIXTURE DO NOT COPY')
            output = root / 'plugins'
            receipt = PLUGIN.generate(binary, root / 'db', output, credential)
            self.assertEqual(receipt['identity'], 'restricted')
            self.assertFalse(receipt['managed_worker'])
            for path in output.rglob('*'):
                if path.is_file():
                    self.assertNotIn('SECRET FIXTURE', path.read_text())
                    if path.suffix in ('.json', '.toml'):
                        self.assertNotIn('--with-worker', path.read_text())

    def test_bad_binary_and_interpolated_paths_leave_no_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root / 'opencut'; binary.touch()
            with self.assertRaises(ValueError):
                PLUGIN.generate(binary, root / 'db', root / 'output')
            binary.chmod(0o700)
            with self.assertRaises(ValueError):
                PLUGIN.generate(binary, root / '${DATABASE}', root / 'output')
            self.assertFalse((root / 'output').exists())
