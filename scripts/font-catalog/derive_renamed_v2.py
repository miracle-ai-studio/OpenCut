#!/usr/bin/env python3
"""Pinned OFL variable instance with explicit reserved-name identity rewrite.

V1 code, locks and outputs remain unchanged. Font/license bytes are caller-owned
fixed inputs; no URL, project text or system font discovery is performed here.
"""
import hashlib
import io
import json
import re
import struct
from pathlib import Path
import derive_static as v1
import generate

REWRITE_VERSION = 'opencut-fixed-opaque-names-v1'
UBUNTU_REWRITE_VERSION = 'opencut-ubuntu-derivative-names-v1'
IDENTITY_IDS = (1, 2, 3, 4, 6, 16, 17, 18, 20, 21, 22, 25)
MAX_BYTES = v1.MAX_BYTES
MAX_SOURCE_BYTES = 64 * 1024 * 1024
canonical = v1.canonical
sha = v1.sha
git_blob = v1.git_blob

def implementation_identity():
    identity = v1.implementation_identity()
    identity['derivation_sha256'] = sha(Path(__file__).read_bytes())
    return identity

def reserved_names(notice):
    header = notice.split('SIL OPEN FONT LICENSE', 1)[0]
    names = []
    for line in header.splitlines():
        match = re.search(r'reserved font names?\s*:?\s*(.+)', line, re.I)
        if not match:
            continue
        value = match.group(1).strip()
        if value[0] in '\"\u201c\u201d\u2018\u2019\'':
            found = re.match(r'[\"\u201c\u201d\u2018\u2019\']([^\"\u201c\u201d\u2018\u2019\']+)[\"\u201c\u201d\u2018\u2019\']', value)
            if not found: raise ValueError('unreviewed reserved-name declaration')
            value = found.group(1)
        else:
            value = value.split('.', 1)[0].strip()
        if not value or len(value) > 256 or any(ord(c) < 32 for c in value):
            raise ValueError('unreviewed reserved-name declaration')
        names.append(value)
    return sorted(set(names))

def opaque_names(source_sha256, axes, weight, style, profile=REWRITE_VERSION, source_family=None):
    token = sha(canonical(dict(profile=profile, source_sha256=source_sha256,
                               axes=axes, weight=weight, style=style)))[:24]
    if profile==UBUNTU_REWRITE_VERSION:
        if source_family not in ('Ubuntu Sans','Ubuntu Sans Mono'):raise ValueError('unreviewed Ubuntu derivative family')
        return source_family+' derivative OCDF '+token,source_family.replace(' ','')+'-derivative-OCDF-'+token
    return 'OCDF ' + token, 'OCDF-' + token

def verify_pins(source, license_bytes, spec):
    # Use original fixed-source/file/license checks. RFN is deliberately handled
    # here, never by deleting the V1 refusal or forging a notice without its RFN.
    if not re.fullmatch('[0-9a-f]{40}',spec['source_commit']): raise ValueError('unfixed source commit')
    if spec['source_repository'] not in ('google/fonts','google/material-design-icons') or spec['license_repository'] not in ('google/fonts','google/material-design-icons') or not re.fullmatch('[0-9a-f]{40}',spec['license_commit']):raise ValueError('unapproved fixed source repository')
    path = Path(spec['source_path'])
    if path.is_absolute() or '..' in path.parts or not 2<=len(path.parts)<=8:
        raise ValueError('renamed profile requires an approved source path')
    if not source or len(source) > MAX_SOURCE_BYTES: raise ValueError('source envelope exceeded')
    if (sha(source), len(source), git_blob(source)) != (spec['source_sha256'], spec['source_size'], spec['source_git_blob']):
        raise ValueError('source identity mismatch')
    if not license_bytes or len(license_bytes) > 65536 or b'\0' in license_bytes: raise ValueError('license envelope exceeded')
    if (sha(license_bytes), git_blob(license_bytes)) != (spec['license_sha256'], spec['license_git_blob']):
        raise ValueError('license identity mismatch')
    notice = license_bytes.decode('utf-8')
    if spec['license_identifier']=='OFL-1.1':
        if 'SIL OPEN FONT LICENSE' not in notice or spec['name_rewrite_version']!=REWRITE_VERSION:raise ValueError('original OFL notice/profile required')
    elif spec['license_identifier']=='Apache-2.0':
        if 'Apache License' not in notice or 'Version 2.0' not in notice or spec['name_rewrite_version']!=REWRITE_VERSION:raise ValueError('original Apache notice/profile required')
    elif spec['license_identifier']=='Ubuntu-font-1.0':
        if 'UBUNTU FONT LICENCE Version 1.0' not in notice or spec['name_rewrite_version']!=UBUNTU_REWRITE_VERSION:raise ValueError('original Ubuntu notice/profile required')
        expected=spec['family']+'; Modified Version: fixed static instance and derivative naming; original outlines retained.'
        if spec.get('derivative_notice')!=expected:raise ValueError('Ubuntu modified-version declaration missing')
    else:raise ValueError('unapproved derivative redistribution license')
    if reserved_names(notice) != spec['reserved_names']: raise ValueError('reserved-name pin differs from original notice')
    if spec['render_profile'] not in ('static_outline','colr_v0','colr_v1','monochrome_bitmap'): raise ValueError('unapproved fixed render profile')
    if spec['recipe_kind'] not in ('instantiate_renamed','static_metadata_corrected','static_checksum_repaired'): raise ValueError('unapproved derivation kind')
    if spec['weight'] not in (400, 700) or spec['style'] not in ('normal', 'italic'): raise ValueError('unsupported target face')
    if spec['name_rewrite_version'] not in (REWRITE_VERSION,UBUNTU_REWRITE_VERSION) or spec['rewritten_name_ids'] != list(IDENTITY_IDS):
        raise ValueError('unapproved name rewrite profile')
    family, postscript = opaque_names(spec['source_sha256'], spec['axes'], spec['weight'], spec['style'],spec['name_rewrite_version'],spec['source_family'])
    if spec['family'] != family or spec['output_postscript_name'] != postscript:
        raise ValueError('output names differ from deterministic opaque recipe')
    if not spec['source_family'] or len(spec['source_family']) > 512: raise ValueError('source display family missing')
    for reserved in spec['reserved_names']:
        if reserved.casefold() in family.casefold() or reserved.casefold() in postscript.casefold():
            raise ValueError('output identity retains a reserved name')

def rewrite_names(font, spec):
    family, ps = spec['family'], spec['output_postscript_name']
    subfamily = { (400,'normal'):'Regular', (700,'normal'):'Bold',
                  (400,'italic'):'Italic', (700,'italic'):'Bold Italic' }[spec['weight'], spec['style']]
    values = {1:family, 2:subfamily, 3:ps+';v2', 4:family+' '+subfamily, 6:ps,
              16:family, 17:subfamily, 18:family+' '+subfamily, 20:ps,
              21:family, 22:subfamily, 25:ps}
    before = []
    table = font['name']
    for record in table.names:
        if record.nameID in IDENTITY_IDS:
            before.append(dict(id=record.nameID, platform=record.platformID,
                               encoding=record.platEncID, language=record.langID,
                               value=record.toUnicode()))
            record.string = values[record.nameID].encode(record.getEncoding(), errors='strict')
    # Stable Unicode/Windows bindings also cover a source missing a legacy name.
    for name_id, value in values.items():
        table.setName(value, name_id, 3, 1, 0x409)
        table.setName(value, name_id, 0, 4, 0)
    if spec['name_rewrite_version']==UBUNTU_REWRITE_VERSION:
        # Keep the original description, copyright, licence and trademark
        # records, while visibly declaring this modified static derivative.
        original=table.getDebugName(10) or ''
        table.setName(original+'\n'+spec['derivative_notice'],10,3,1,0x409)
    # A static instance has no remaining variation-name selection. Remove stale
    # source instance labels and STAT, retaining copyright/trademark/license IDs.
    table.names = [n for n in table.names if n.nameID < 256]
    dropped = []
    for tag in ('STAT', 'DSIG'):
        if tag in font: del font[tag]; dropped.append(tag)
    if 'CFF ' in font:
        cff = font['CFF '].cff
        cff.fontNames = [ps for _ in cff.fontNames]
        for top in cff.topDictIndex:
            top.FamilyName = family; top.FullName = family+' '+subfamily
            if hasattr(top, 'FontName'): top.FontName = ps
            if hasattr(top, 'FDArray'):
                for index, fd in enumerate(top.FDArray):
                    fd.FontName = ps+'-FD'+str(index)
    manifest = sorted([dict(id=n.nameID, platform=n.platformID, encoding=n.platEncID,
                            language=n.langID, value=n.toUnicode())
                       for n in table.names if n.nameID in IDENTITY_IDS],
                      key=lambda n:(n['id'],n['platform'],n['encoding'],n['language'],n['value']))
    if any(values[n['id']] != n['value'] for n in manifest): raise ValueError('identity name rewrite incomplete')
    return dict(profile=spec['name_rewrite_version'], original_identity_sha256=sha(canonical(sorted(before,key=lambda n:(n['id'],n['platform'],n['encoding'],n['language'],n['value'])))),
                output_identity_names=manifest, removed_tables=dropped, removed_custom_name_ids=True,
                cff_identity_rewritten='CFF ' in font)

def reassemble_static(source, rewritten, allowed):
    """Retain original SFNT payloads outside the reviewed metadata tables.

    TTFont may normalize an unrelated table during a save. Static correction
    does not authorize that normalization, even when it is semantically benign.
    Rebuild the directory/checksums while copying protected payloads verbatim.
    """
    from fontTools.ttLib.sfnt import SFNTReader, SFNTWriter
    original=SFNTReader(io.BytesIO(source));changed=SFNTReader(io.BytesIO(rewritten))
    tags=set(original.keys())|set(changed.keys())
    tables={tag:(changed[tag] if tag in allowed else original[tag])
            for tag in tags if tag not in allowed or tag in changed}
    output=io.BytesIO();writer=SFNTWriter(output,len(tables),original.sfntVersion)
    for tag in sorted(tables):writer[tag]=tables[tag]
    writer.close()
    return output.getvalue()

def derive(source, license_bytes, spec, tool_lock):
    verify_pins(source, license_bytes, spec)
    actual_tool = implementation_identity()
    if actual_tool != tool_lock: raise ValueError('instancer implementation identity mismatch')
    from fontTools.ttLib import TTFont
    from fontTools.varLib.instancer import instantiateVariableFont
    correction=None;source_tables=None;protected_source=None
    with TTFont(io.BytesIO(source), recalcTimestamp=False, recalcBBoxes=False, lazy=False) as font:
        if spec['recipe_kind']=='instantiate_renamed':
            if 'fvar' not in font: raise ValueError('source is not variable')
            axes = v1.axis_coordinates(font, spec['axes'])
            font['head'].created = font['head'].modified = v1.EPOCH
            instance = instantiateVariableFont(font, axes, inplace=False, optimize=True, updateFontNames=False)
        else:
            if 'fvar' in font or spec['axes']:raise ValueError('static correction cannot impersonate a variable instance')
            before=static_metadata(font)
            if before!=spec['original_metadata']:raise ValueError('static correction original metadata differs from reviewed recipe')
            allowed=['head','name','STAT','DSIG']
            source_tables={tag:sha(font.reader[tag]) for tag in font.reader.keys()}
            if spec['recipe_kind']=='static_metadata_corrected':
                if (before['weight'],before['style'])==(spec['weight'],spec['style']):raise ValueError('metadata correction must describe a real mismatch')
                allowed.extend(['OS/2','post'])
                os2=font['OS/2'];os2.usWeightClass=spec['weight']
                italic=spec['style']=='italic'
                os2.fsSelection=(os2.fsSelection&~(1|32|64))|(1 if italic else 0)|(32 if spec['weight']==700 else (0 if italic else 64))
                head=font['head'];head.macStyle=(head.macStyle&~3)|(1 if spec['weight']==700 else 0)|(2 if italic else 0)
                # The reviewed Geo oblique target retains its source angle;
                # normal targets remove only the incorrect italic metadata.
                font['post'].italicAngle=before['italic_angle'] if italic else 0
            elif (before['weight'],before['style'])!=(spec['weight'],spec['style']):raise ValueError('checksum repair cannot change style or weight')
            protected_source={tag:hash for tag,hash in source_tables.items() if tag not in allowed}
            correction=dict(original_metadata=before,target_weight=spec['weight'],target_style=spec['style'],allowed_changed_tables=allowed,preserved_source_tables=protected_source)
            instance=font
        try:
            converted = 'CFF2' in instance
            if converted:
                from fontTools.cffLib.CFF2ToCFF import convertCFF2ToCFF
                convertCFF2ToCFF(instance)
            # Compile once before the rewrite: fontTools table compilation may
            # update dependent metrics (glyf->loca/hmtx/hhea). Compare actual
            # SFNT table bytes across the name-only rewrite, not mutable caches.
            instance.recalcTimestamp=False
            intermediate=io.BytesIO();instance.save(intermediate,reorderTables=True)
            instance.close();instance=TTFont(io.BytesIO(intermediate.getvalue()),recalcTimestamp=False,lazy=False)
            stable = {tag:sha(instance.reader[tag]) for tag in instance.reader.keys()
                      if tag not in ('head','name','STAT','DSIG','CFF ')}
            names = rewrite_names(instance, spec)
            instance.recalcTimestamp = False
            instance['head'].created = instance['head'].modified = v1.EPOCH
            output = io.BytesIO(); instance.save(output, reorderTables=True); data = output.getvalue()
            if correction:data=reassemble_static(source,data,correction['allowed_changed_tables'])
            with TTFont(io.BytesIO(data),recalcTimestamp=False,lazy=True) as finished:
                if correction:stable={tag:sha(finished.reader[tag]) for tag in stable}
                elif stable != {tag:sha(finished.reader[tag]) for tag in stable}: raise ValueError('identity rewrite changed instantiated SFNT tables')
                profile=spec['render_profile']
                color='COLR' in finished
                bitmap=any(tag in finished for tag in ('EBDT','bdat'))
                if profile=='static_outline' and (color or bitmap or 'SVG ' in finished): raise ValueError('source requires a different explicit render profile')
                if profile.startswith('colr_') and (not color or 'CPAL' not in finished or finished['COLR'].version!=int(profile[-1])): raise ValueError('COLR profile differs from fixed output tables')
                if profile=='monochrome_bitmap' and (not bitmap or color or 'SVG ' in finished): raise ValueError('bitmap profile differs from fixed output tables')
                native_family=finished['name'].getBestFamilyName()
                os2=finished['OS/2'];native_style='italic' if os2.fsSelection&1 or finished['post'].italicAngle else 'normal'
                if (native_family,os2.usWeightClass,native_style)!=(spec['family'],spec['weight'],spec['style']):raise ValueError('derived face identity differs from opaque target')
                if correction:
                    if protected_source!={tag:sha(finished.reader[tag]) for tag in protected_source}:raise ValueError('static correction changed a protected source table')
                    correction['changed_tables']={tag:dict(before=source_tables[tag],after=sha(finished.reader[tag]) if tag in finished.reader else None)
                                                  for tag in source_tables if tag not in finished.reader or source_tables[tag]!=sha(finished.reader[tag])}
        finally: instance.close()
    if len(data) > MAX_BYTES: raise ValueError('derived font envelope exceeded')
    if spec['render_profile']=='static_outline' and generate.face(data) != (spec['family'], spec['weight'], spec['style']): raise ValueError('derived static face does not match exact target')
    evidence = dict(schema_version=2, source=dict(spec), tool=actual_tool,
                    output_sha256=sha(data), output_size=len(data), license_notice=license_bytes.decode('utf-8'),
                    name_rewrite=names, cff2_to_cff=converted, preserved_geometry_tables=stable)
    if correction:evidence['static_correction']=correction
    evidence['derivation_id'] = 'font-derived-v2_' + sha(canonical(evidence))
    return data, evidence

def static_metadata(font):
    os2=font.reader['OS/2'];post=font.reader['post'];head=font.reader['head']
    flags=struct.unpack_from('>H',os2,62)[0];angle=struct.unpack_from('>i',post,4)[0]/65536
    return dict(family=font['name'].getBestFamilyName(),weight=struct.unpack_from('>H',os2,4)[0],
                style='italic' if flags&1 or angle else 'normal',fs_selection=flags,
                mac_style=struct.unpack_from('>H',head,44)[0],italic_angle=angle)
