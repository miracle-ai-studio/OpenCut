"""Wire projection preserves exact canonical identity, license and coordinates."""
import unittest
import tempfile
from pathlib import Path
import derive_static as d
from generate_derived_catalog import wire, publish_cache, coverage_report

class CatalogTests(unittest.TestCase):
    def test_proof_and_axis_number_spellings_survive_projection(self):
        proof=dict(source=dict(family='Fixture',weight=700,style='italic',source_commit=d.generate.COMMIT,source_path='ofl/fixture/Fixture-Italic[wght].ttf',source_sha256='a'*64,source_size=123,license_path='ofl/fixture/OFL.txt',license_identifier='OFL-1.1',axes={'wght':700,'wdth':100.0}),tool={'fixed':'identity'},output_sha256='b'*64,output_size=45,license_notice='Complete original notice\n')
        proof['derivation_id']='font-derived_'+d.sha(d.canonical(proof))
        result=wire(proof)
        self.assertEqual(result['face']['id'],'google-fixture-bold-italic-derived-v1')
        self.assertEqual(result['face']['derivation_id'],proof['derivation_id'])
        self.assertEqual(result['proof_json'].encode(),d.canonical(proof))
        self.assertEqual(result['axes'],[dict(tag='wdth',coordinate='100.0'),dict(tag='wght',coordinate='700')])
        self.assertEqual(result['license_notice'],proof['license_notice'])
        self.assertEqual(result['source_size_bytes'],'123')

    def test_coverage_counts_expressed_faces_and_keeps_gaps_explicit(self):
        atlas={'Fixture':{'s':['300','400','700','400i']},'Missing':{'s':['400']}}
        present={('Fixture',400,'normal')}
        pins=[{'face':dict(family='Fixture',weight=700,style='normal')}]
        result=coverage_report(dict(failures=[]),pins,atlas,present,b'[]')
        self.assertEqual(result['combined_pinned_applicable_faces'],2)
        self.assertEqual(result['applicable_classic_faces'],4)
        self.assertEqual(result['unexpressed_classic_face_declarations'],1)
        self.assertEqual(result['covered_400_normal_families'],1)
        self.assertEqual(result['remaining_400_normal_families'],['Missing'])
        self.assertEqual(len(result['missing_applicable_faces']),2)
        self.assertTrue(all(f['reason']=='no_reviewed_pinned_recipe' for f in result['missing_applicable_faces']))

    def test_output_cache_is_hash_addressed_and_refuses_changed_bytes(self):
        data=b'fixed static font';proof=dict(output_size=len(data),output_sha256=d.sha(data))
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);publish_cache(base,data,proof)
            self.assertEqual((base/'derived'/proof['output_sha256']).read_bytes(),data)
            with self.assertRaisesRegex(ValueError,'identity mismatch'):
                publish_cache(base,b'changed bytes',proof)
            self.assertEqual((base/'derived'/proof['output_sha256']).read_bytes(),data)

if __name__=='__main__': unittest.main()
