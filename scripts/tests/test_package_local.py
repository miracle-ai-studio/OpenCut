"""Delivery must exclude local residue and reject undeclared package contents."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_local", Path(__file__).resolve().parents[1] / "package-local.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackageContentsTest(unittest.TestCase):
    def test_packet_runtime_copies_only_pinned_files_and_rejects_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'packages'
            audio = source / 'classic-audio-renderer'
            art = source / 'classic-source-art-renderer'
            media = audio / 'node_modules/mediabunny'
            manifests = {
                audio: {'name': '@opencut/classic-audio-renderer', 'version': '0.1.0', 'type': 'module',
                        'dependencies': {'mediabunny': '1.41.0', 'playwright-core': '1.63.0'}},
                art: {'name': '@opencut/classic-source-art-renderer', 'version': '0.1.0', 'type': 'module'},
                media: {'name': 'mediabunny', 'version': '1.41.0', 'type': 'module', 'license': 'MPL-2.0'},
            }
            for folder, manifest in manifests.items():
                folder.mkdir(parents=True, exist_ok=True)
                (folder / 'package.json').write_text(json.dumps(manifest))
            for folder, relative in [(media, 'dist/bundles/mediabunny.mjs'), (media, 'LICENSE'),
                                     (art, 'host/packet-rate.mjs'), (art, 'host/browser.mjs')]:
                file = folder / relative
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text('fixture')
            destination = root / 'bin/libexec'
            scope = package.copy_packet_rate_runtime(source, destination)
            copied = {str(p.relative_to(destination)) for p in destination.rglob('*') if p.is_file()}
            self.assertEqual(len(copied), 6)
            self.assertNotIn('classic-source-art-renderer/host/browser.mjs', copied)
            self.assertEqual(scope['profile'], 'classic-packet-rate-v1-node-only')
            self.assertEqual(scope['external_runtime'], 'node')
            self.assertEqual(scope['library_license'], 'MPL-2.0')
            self.assertEqual(package.sha(destination / 'classic-source-art-renderer/host/packet-rate.mjs'),
                             package.sha(art / 'host/packet-rate.mjs'))
            (media / 'package.json').write_text(json.dumps({**manifests[media], 'version': '1.42.0'}))
            with self.assertRaisesRegex(ValueError, 'Unpinned'):
                package.copy_packet_rate_runtime(source, root / 'wrong-version')
            (media / 'package.json').write_text(json.dumps(manifests[media]))
            (art / 'host/packet-rate.mjs').unlink()
            (art / 'host/packet-rate.mjs').symlink_to(art / 'host/browser.mjs')
            with self.assertRaisesRegex(ValueError, 'Symlink'):
                package.copy_packet_rate_runtime(source, root / 'linked-file')
            (art / 'host/packet-rate.mjs').unlink()
            (art / 'host/packet-rate.mjs').write_text('fixture')
            (audio / 'node_modules').rename(root / 'outside-modules')
            (audio / 'node_modules').symlink_to(root / 'outside-modules', target_is_directory=True)
            with self.assertRaisesRegex(ValueError, 'Symlink'):
                package.copy_packet_rate_runtime(source, root / 'linked-directory')

    def test_rust_source_retains_test_modules_but_excludes_local_residue(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src"
            source.mkdir()
            for name in ["lib.rs", "tests.rs", ".DS_Store", "lib.rs.bak"]:
                (source / name).write_text("fixture")
            (source / "target").mkdir()
            (source / "target" / "private.log").write_text("not distributable")
            package.copy_sdk_sources(source, root / "copy")
            self.assertEqual(sorted(p.name for p in (root / "copy").iterdir()), ["lib.rs", "tests.rs"])

    def test_rust_source_rejects_unreviewed_files_and_symlinks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src"
            source.mkdir()
            (source / "workspace.sqlite").write_bytes(b"private")
            with self.assertRaisesRegex(ValueError, "Unreviewed"):
                package.copy_sdk_sources(source, root / "copy")
            (source / "workspace.sqlite").unlink()
            (source / "lib.rs").symlink_to(root / "outside")
            with self.assertRaisesRegex(ValueError, "Symlink"):
                package.copy_sdk_sources(source, root / "second")

    def test_npm_delivery_rejects_tests_caches_and_unexpected_files(self):
        package.validate_npm_files([{"path":p} for p in ["package.json", "README.md", "dist/index.js", "dist/index.d.ts"]])
        for name in ["tests/transport.test.mjs", "dist/index.test.js", "dist/index.spec.d.ts", "dist/.DS_Store", "dist/nested/private.js", "node_modules/module.js"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                package.validate_npm_files([{"path":name}])

    def test_web_delivery_keeps_public_assets_and_rejects_private_residue(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'build'
            (source / 'assets').mkdir(parents=True)
            (source / 'index.html').write_text('<title>Editor</title>')
            (source / 'assets/editor.js').write_text('export const editor = true')
            (source / 'assets/classic-exporter.js').write_text('void (() => {})()')
            package.copy_web_build(source, root / 'web')
            self.assertTrue((root / 'web/assets/editor.js').is_file())
            self.assertEqual((root / 'web/assets/classic-exporter.js').read_bytes(), (source / 'assets/classic-exporter.js').read_bytes())
            (source / 'unreviewed.js').write_text('private')
            with self.assertRaisesRegex(ValueError, 'Unreviewed Web'):
                package.copy_web_build(source, root / 'unreviewed')
            (source / 'unreviewed.js').unlink()
            (source / 'api-token').write_text('private')
            with self.assertRaisesRegex(ValueError, 'Unreviewed Web'):
                package.copy_web_build(source, root / 'rejected')
