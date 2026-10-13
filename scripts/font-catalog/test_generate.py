"""Offline discovery fixtures. No regeneration or network access."""
import importlib.util
from pathlib import Path
import unittest
spec = importlib.util.spec_from_file_location('font_catalog_generate', Path(__file__).with_name('generate.py'))
generate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generate)

class StaticDiscoveryTests(unittest.TestCase):
    def test_nonstandard_filename_and_static_directory_are_discovered(self):
        paths = {p: dict(type='blob', size=42) for p in (
            'ofl/candal/Candal.ttf', 'ofl/candal/OFL.txt',
            'ofl/inconsolata/static/Inconsolata-Regular.ttf', 'ofl/inconsolata/OFL.txt')}
        jobs = generate.static_jobs({'Candal': {'s': ['400']}, 'Inconsolata': {'s': ['400']}}, paths)
        self.assertEqual({j[1] for j in jobs}, {'ofl/candal/Candal.ttf', 'ofl/inconsolata/static/Inconsolata-Regular.ttf'})
        self.assertTrue(all(j[3] is None for j in jobs))  # metadata, not name, determines style

    def test_variable_unsupported_license_and_arbitrary_subtrees_stay_excluded(self):
        paths = {p: dict(type='blob', size=42) for p in (
            'ofl/candal/Candal[wght].ttf', 'ofl/candal/OFL.txt',
            'ofl/candal/archive/Candal.ttf', 'ufl/candal/Candal.ttf', 'ufl/candal/LICENSE.txt')}
        self.assertEqual(generate.static_jobs({'Candal': {'s': ['400']}}, paths), [])

    def test_original_license_is_required_and_selection_is_deterministic(self):
        paths = {'ofl/candal/Candal.ttf': dict(type='blob', size=42)}
        self.assertEqual(generate.static_jobs({'Candal': {'s': ['400']}}, paths), [])
        paths['ofl/candal/OFL.txt'] = dict(type='blob', size=12)
        self.assertEqual(generate.static_jobs({'Candal': {'s': ['400']}}, paths), generate.static_jobs({'Candal': {'s': ['400']}}, dict(reversed(list(paths.items())))))

class StableIdentityTests(unittest.TestCase):
    def test_original_root_face_wins_new_path_collision(self):
        original = dict(family='Candal', weight=400, style='normal', path='ofl/candal/Z-Regular.ttf', content_hash='old')
        discovered = dict(family='Candal', weight=400, style='normal', path='ofl/candal/A.ttf', content_hash='new')
        nested = dict(family='Candal', weight=400, style='normal', path='ofl/candal/static/A-Regular.ttf', content_hash='nested')
        self.assertEqual(generate.select_records([nested, discovered, original]), [original])
        self.assertEqual(generate.select_records([original, discovered, nested]), [original])

    def test_mislabeled_original_regular_cannot_become_italic(self):
        with self.assertRaisesRegex(ValueError, 'declared weight/style'):
            generate.admitted_style((400, 'normal'), 400, 'italic')
        self.assertEqual(generate.admitted_style(None, 400, 'italic'), (400, 'italic'))
        self.assertEqual(generate.admitted_style((700, 'italic'), 700, 'italic'), (700, 'italic'))

if __name__ == '__main__': unittest.main()
