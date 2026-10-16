"""Directed V2 recipe/name identity tests. No fonts, network or system lookup."""
import importlib.util
import unittest
from types import SimpleNamespace
import derive_renamed_v2 as d

NOTICE=b'Copyright Original Authors, with Reserved Font Name "Source".\nSIL OPEN FONT LICENSE Version 1.1\nOriginal full notice retained.'
def spec(notice=NOTICE):
    source=b'fixed-source';axes={'wght':400};family,ps=d.opaque_names(d.sha(source),axes,400,'normal')
    return dict(source_repository='google/fonts',source_commit=d.generate.COMMIT,source_path='ofl/fixture/Fixture[wght].ttf',source_sha256=d.sha(source),source_size=len(source),source_git_blob=d.git_blob(source),license_repository='google/fonts',license_commit=d.generate.COMMIT,license_path='ofl/fixture/OFL.txt',license_sha256=d.sha(notice),license_git_blob=d.git_blob(notice),license_identifier='OFL-1.1',family=family,source_family='Original Display',weight=400,style='normal',axes=axes,name_rewrite_version=d.REWRITE_VERSION,output_postscript_name=ps,reserved_names=d.reserved_names(notice.decode()),rewritten_name_ids=list(d.IDENTITY_IDS),recipe_kind='instantiate_renamed',render_profile='static_outline')

class RecipeTests(unittest.TestCase):
    def test_original_reserved_notice_is_retained_and_does_not_need_forged_v1_notice(self):
        value=spec();d.verify_pins(b'fixed-source',NOTICE,value)
        self.assertEqual(value['reserved_names'],['Source']);self.assertNotIn('Source',value['family'])
        with self.assertRaisesRegex(ValueError,'reserved font name'):
            d.v1.verify_pins(b'fixed-source',NOTICE,value)
        with self.assertRaisesRegex(ValueError,'license identity'):
            d.verify_pins(b'fixed-source',NOTICE+b'changed',value)
    def test_quoted_curly_unquoted_and_multiple_original_reserved_declarations(self):
        notice='Copyright A, Reserved Font Name “Family One”.\nCopyright B, with Reserved Font Name: Family Two.\nSIL OPEN FONT LICENSE'
        self.assertEqual(d.reserved_names(notice),['Family One','Family Two'])
    def test_changed_names_profile_axis_and_unpinned_rfn_are_rejected(self):
        for field,value in [('family','Source'),('output_postscript_name','Source-Regular'),('render_profile','arbitrary-svg'),('name_rewrite_version','unknown'),('recipe_kind','unreviewed-fix'),('reserved_names',[])]:
            changed=spec();changed[field]=value
            with self.assertRaises(ValueError):d.verify_pins(b'fixed-source',NOTICE,changed)
        changed=spec();changed['axes']={'wght':700}
        with self.assertRaisesRegex(ValueError,'deterministic'):d.verify_pins(b'fixed-source',NOTICE,changed)
    def test_no_rfn_color_recipe_still_requires_explicit_opaque_names_and_profile(self):
        notice=b'Copyright Original Authors\nSIL OPEN FONT LICENSE Version 1.1';value=spec(notice);value['render_profile']='colr_v1'
        d.verify_pins(b'fixed-source',notice,value);self.assertEqual(value['reserved_names'],[])
    def test_ubuntu_derivative_keeps_original_name_as_licence_2c_requires(self):
        family,ps=d.opaque_names('a'*64,{'wght':400},400,'normal',d.UBUNTU_REWRITE_VERSION,'Ubuntu Sans')
        self.assertTrue(family.startswith('Ubuntu Sans derivative OCDF '));self.assertTrue(ps.startswith('UbuntuSans-derivative-OCDF-'))
        opaque,_=d.opaque_names('a'*64,{'wght':400},400,'normal')
        self.assertTrue(opaque.startswith('OCDF '));self.assertNotEqual(opaque,family)

@unittest.skipUnless(importlib.util.find_spec('fontTools'),'fixed fontTools unavailable')
class NameRewriteTests(unittest.TestCase):
    def test_static_reassembly_keeps_original_protected_payloads_and_rebuilds_checksums(self):
        from fontTools.ttLib.sfnt import SFNTReader, SFNTWriter, calcChecksum
        def sfnt(tables):
            data=d.io.BytesIO();writer=SFNTWriter(data,len(tables))
            for tag,value in sorted(tables.items()):writer[tag]=value
            writer.close();return data.getvalue()
        head=b'\0\1\0\0'+b'\0'*50
        source=sfnt({'head':head,'name':b'old-name','OS/2':b'old-metadata','glyf':b'original-glyph-payload','hmtx':b'original-metrics'})
        rewritten=sfnt({'head':head,'name':b'opaque-name','OS/2':b'fixed-metadata','glyf':b'incidental-recompile','hmtx':b'incidental-normalization'})
        output=d.reassemble_static(source,rewritten,['head','name','OS/2','post','STAT','DSIG'])
        reader=SFNTReader(d.io.BytesIO(output));self.assertEqual(reader['glyf'],b'original-glyph-payload');self.assertEqual(reader['hmtx'],b'original-metrics')
        self.assertEqual(reader['name'],b'opaque-name');self.assertEqual(reader['OS/2'],b'fixed-metadata');self.assertEqual(calcChecksum(output),0xB1B0AFBA)

    def test_all_name_ps_cff_bindings_change_but_legal_notice_records_do_not(self):
        from fontTools.ttLib import newTable
        table=newTable('name');table.names=[]
        for name_id in d.IDENTITY_IDS:table.setName('Source Original',name_id,3,1,0x409)
        table.setName('Source Copyright',0,3,1,0x409);table.setName(NOTICE.decode(),13,3,1,0x409);table.setName('https://original.invalid/license',14,3,1,0x409);table.setName('Source Instance',256,3,1,0x409)
        fd=SimpleNamespace(FontName='SourceFD');top=SimpleNamespace(FamilyName='Source',FullName='Source Original',FontName='SourcePS',FDArray=[fd])
        cff=SimpleNamespace(fontNames=['SourcePS'],topDictIndex=[top]);font={'name':table,'CFF ':SimpleNamespace(cff=cff),'STAT':object(),'DSIG':object()}
        value=spec();proof=d.rewrite_names(font,value)
        self.assertEqual(table.getDebugName(0),'Source Copyright');self.assertEqual(table.getDebugName(13),NOTICE.decode());self.assertEqual(table.getDebugName(14),'https://original.invalid/license')
        self.assertNotIn('STAT',font);self.assertNotIn('DSIG',font);self.assertFalse(any(n.nameID>=256 for n in table.names))
        self.assertEqual(cff.fontNames,[value['output_postscript_name']]);self.assertEqual(top.FamilyName,value['family']);self.assertEqual(top.FontName,value['output_postscript_name']);self.assertEqual(fd.FontName,value['output_postscript_name']+'-FD0')
        self.assertTrue(all('Source' not in n['value'] for n in proof['output_identity_names']));self.assertTrue(proof['cff_identity_rewritten'])
        self.assertEqual({n['id'] for n in proof['output_identity_names']},set(d.IDENTITY_IDS))

if __name__=='__main__':unittest.main()
