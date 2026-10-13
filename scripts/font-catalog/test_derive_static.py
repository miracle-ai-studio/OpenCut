"""Bounded offline pin/axis rejection tests plus real synthetic VF round-trip."""
import importlib.util
import unittest
from types import SimpleNamespace
import derive_static as d

def spec(source=b'source', license_bytes=b'Copyright Fixture\nSIL OPEN FONT LICENSE Version 1.1'):
    return dict(source_commit=d.generate.COMMIT, source_path='ofl/fixture/Fixture[wght].ttf',
                source_sha256=d.sha(source), source_size=len(source), source_git_blob=d.git_blob(source),
                license_path='ofl/fixture/OFL.txt', license_sha256=d.sha(license_bytes), license_git_blob=d.git_blob(license_bytes),
                license_identifier='OFL-1.1', family='Fixture', weight=400, style='normal', axes={'wght':400})

class PinTests(unittest.TestCase):
    def test_changed_source_and_license_refused(self):
        s=spec(); notice=b'Copyright Fixture\nSIL OPEN FONT LICENSE Version 1.1'
        d.verify_pins(b'source',notice,s)
        for source,license_bytes in [(b'changed',notice),(b'source',notice+b'changed')]:
            with self.assertRaisesRegex(ValueError,'identity mismatch'): d.verify_pins(source,license_bytes,s)
    def test_reserved_name_and_unknown_license_refused(self):
        notice=b'Copyright Fixture. Reserved Font Name Fixture\nSIL OPEN FONT LICENSE Version 1.1'
        with self.assertRaisesRegex(ValueError,'reserved font name'): d.verify_pins(b'source',notice,spec(license_bytes=notice))
        s=spec();s['license_identifier']='UFL'
        with self.assertRaisesRegex(ValueError,'unsupported'):d.verify_pins(b'source',b'Copyright Fixture\nSIL OPEN FONT LICENSE Version 1.1',s)
    def test_every_axis_explicit_finite_and_in_range(self):
        font={'fvar':SimpleNamespace(axes=[SimpleNamespace(axisTag='wght',minValue=100,maxValue=900),SimpleNamespace(axisTag='wdth',minValue=75,maxValue=125)])}
        for coords in ({'wght':400},{'wght':400,'wdth':None},{'wght':400,'wdth':float('nan')},{'wght':400,'wdth':126},{'wght':400,'wdth':100,'opsz':12}):
            with self.assertRaises(ValueError): d.axis_coordinates(font,coords)
        self.assertEqual(d.axis_coordinates(font,{'wdth':100,'wght':400}),{'wdth':100,'wght':400})

@unittest.skipUnless(importlib.util.find_spec('fontTools'), 'fixed fontTools environment unavailable')
class SyntheticVariableTests(unittest.TestCase):
    def test_complete_instance_deterministic_and_native_metadata_compatible(self):
        import io
        from fontTools.fontBuilder import FontBuilder
        from fontTools.pens.ttGlyphPen import TTGlyphPen
        from fontTools.ttLib import newTable
        from fontTools.ttLib.tables._f_v_a_r import Axis
        builder=FontBuilder(1000,isTTF=True)
        builder.setupGlyphOrder(['.notdef','A'])
        glyphs={}
        for name in ['.notdef','A']:
            pen=TTGlyphPen(None);pen.moveTo((0,0));pen.lineTo((200,0));pen.lineTo((100,500));pen.closePath();glyphs[name]=pen.glyph()
        builder.setupCharacterMap({65:'A'});builder.setupGlyf(glyphs)
        builder.setupHorizontalMetrics({name:(500,0) for name in glyphs});builder.setupHorizontalHeader(ascent=800,descent=-200)
        builder.setupNameTable({'familyName':'Fixture','styleName':'Regular','uniqueFontIdentifier':'Fixture-Regular','fullName':'Fixture Regular','psName':'Fixture-Regular'})
        builder.setupOS2(usWeightClass=400,sTypoAscender=800,sTypoDescender=-200,usWinAscent=800,usWinDescent=200)
        builder.setupPost();builder.setupMaxp()
        fvar=newTable('fvar');axis=Axis();axis.axisTag='wght';axis.minValue=100;axis.defaultValue=400;axis.maxValue=900;axis.flags=0;axis.axisNameID=256;fvar.axes=[axis];fvar.instances=[];builder.font['fvar']=fvar
        from fontTools.ttLib.tables.TupleVariation import TupleVariation
        gvar=newTable('gvar');gvar.variations={'.notdef':[], 'A':[TupleVariation({'wght':(0,1,1)},[(0,0),(100,0),(50,0),(0,0),(0,0),(0,0),(0,0)])]};builder.font['gvar']=gvar
        builder.font.recalcTimestamp=False;builder.font['head'].created=builder.font['head'].modified=d.EPOCH
        stream=io.BytesIO();builder.font.save(stream);source=stream.getvalue()
        notice=b'Copyright Fixture\nSIL OPEN FONT LICENSE Version 1.1'
        s=spec(source,notice);lock=d.implementation_identity()
        one,proof=d.derive(source,notice,s,lock);two,again=d.derive(source,notice,s,lock)
        self.assertEqual(one,two);self.assertEqual(proof,again)
        self.assertEqual(d.generate.face(one),('Fixture',400,'normal'))
        bold_spec=dict(s,weight=700,axes={'wght':700})
        bold,bold_proof=d.derive(source,notice,bold_spec,lock)
        self.assertEqual(d.generate.face(bold),('Fixture',700,'normal'))
        self.assertNotEqual(one,bold)
        from fontTools.ttLib import TTFont
        with TTFont(io.BytesIO(one)) as regular_font, TTFont(io.BytesIO(bold)) as bold_font:
            self.assertNotEqual(regular_font['glyf']['A'].coordinates,bold_font['glyf']['A'].coordinates)
        bad=dict(lock);bad['fonttools_version']='different'
        with self.assertRaisesRegex(ValueError,'implementation identity'):d.derive(source,notice,s,bad)

if __name__=='__main__':unittest.main()
