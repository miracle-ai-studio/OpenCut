import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('plugin_package', Path(__file__).resolve().parents[1] / 'package-plugin.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PluginPackageTests(unittest.TestCase):
    def fixture(self, archive, content=b'fixture executable', extra=None):
        digest = hashlib.sha256(content).hexdigest()
        manifest = {'target': 'x86_64-apple-darwin', 'binary_sha256': digest,
                    'external_dependencies': ['node', 'ffmpeg', 'ffprobe', 'chromium', 'python'],
                    'files': {'bin/opencut': {'size': len(content), 'sha256': digest}}, 'links': {}}
        with tarfile.open(archive, 'w:gz') as tar:
            for name, data in [('host/bin/opencut', content), ('host/manifest.json', json.dumps(manifest).encode()), *(extra or [])]:
                item = tarfile.TarInfo(name); item.size = len(data); tar.addfile(item, io.BytesIO(data))

    def test_portable_archive_and_non_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); runtime = root / 'runtime.tar.gz'; self.fixture(runtime)
            result = package.package(runtime, root / 'out')
            with zipfile.ZipFile(result['archive']) as archive:
                self.assertIn('opencut/plugin.json', archive.namelist())
                self.assertIn('opencut/mcp.json', archive.namelist())
                self.assertIn('opencut/README.zh-CN.md', archive.namelist())
                self.assertIn('opencut/runtime.tar.gz', archive.namelist())
                self.assertTrue(all(name.startswith('opencut/') for name in archive.namelist()))
            with self.assertRaises(FileExistsError): package.package(runtime, root / 'out')

    def test_private_binary_paths_and_traversal_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary) / 'runtime.tar.gz'
            self.fixture(runtime, b'/Users/' + b'fixture-owner/private.rs')
            with self.assertRaisesRegex(ValueError, 'hygiene'): package.inspect_runtime(runtime)
            self.fixture(runtime, extra=[('host/../escape', b'bad')])
            with self.assertRaisesRegex(ValueError, 'Unsafe'): package.inspect_runtime(runtime)

    def test_missing_inventory_member_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            runtime = Path(temporary) / 'runtime.tar.gz'
            self.fixture(runtime, extra=[('host/extra.txt', b'undeclared')])
            with self.assertRaisesRegex(ValueError, 'identity'): package.inspect_runtime(runtime)

    def test_public_dependency_template_does_not_hide_other_home_paths(self):
        name = 'bin/libexec/classic-audio-renderer/node_modules/playwright-core/lib/coreBundle.js'
        public = b'/home/' + b'pwuser/.cache/ms-playwright/webkit'
        self.assertFalse(package.publication_findings(name, public))
        self.assertTrue(package.publication_findings(name, public + b' /Users/' + b'fixture-owner/source.rs'))
