"""Delivery must exclude local residue and reject undeclared package contents."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_local", Path(__file__).resolve().parents[1] / "package-local.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackageContentsTest(unittest.TestCase):
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
            package.copy_web_build(source, root / 'web')
            self.assertTrue((root / 'web/assets/editor.js').is_file())
            (source / 'api-token').write_text('private')
            with self.assertRaisesRegex(ValueError, 'Unreviewed Web'):
                package.copy_web_build(source, root / 'rejected')
