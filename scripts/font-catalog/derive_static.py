#!/usr/bin/env python3
"""Offline, fully pinned variable-to-static derivation; no catalog publication.

Call derive(source, license_bytes, specification, tool_lock). Both specifications
are reviewable JSON. A successful record is evidence, never a runtime grant.
"""
import hashlib
import io
import json
import math
import platform
from pathlib import Path
import generate

EPOCH = 2082844800  # 1970-01-01 in OpenType's 1904 epoch
MAX_BYTES = 32 * 1024 * 1024

def sha(data): return hashlib.sha256(data).hexdigest()
def git_blob(data): return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()
def canonical(value): return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()

def implementation_identity():
    import fontTools
    distribution = json.loads(Path(__file__).with_name("fonttools-lock.json").read_text())
    if fontTools.__version__ != distribution["version"]: raise ValueError("fontTools version differs from fixed wheel")
    root = Path(fontTools.__file__).parent
    # Bind every Python implementation file, not only the top-level instancer.
    files = [(p.relative_to(root).as_posix(), sha(p.read_bytes())) for p in sorted(root.rglob('*.py'))]
    return dict(fonttools_wheel_sha256=distribution["wheel_sha256"], fonttools_version=fontTools.__version__, fonttools_implementation_sha256=sha(canonical(files)),
                python_version=platform.python_version(), derivation_sha256=sha(Path(__file__).read_bytes()),
                static_validator_sha256=sha(Path(generate.__file__).read_bytes()))

def verify_pins(source, license_bytes, spec):
    if spec['source_commit'] != generate.COMMIT: raise ValueError('unfixed Google source commit')
    path = Path(spec['source_path'])
    if path.is_absolute() or '..' in path.parts or len(path.parts) != 3 or path.parts[0] not in ('ofl', 'apache'):
        raise ValueError('unapproved source path')
    if not source or len(source) > MAX_BYTES: raise ValueError('source envelope exceeded')
    if (sha(source), len(source), git_blob(source)) != (spec['source_sha256'], spec['source_size'], spec['source_git_blob']):
        raise ValueError('source identity mismatch')
    if not license_bytes or len(license_bytes) > 65536 or b'\0' in license_bytes: raise ValueError('license envelope exceeded')
    if (sha(license_bytes), git_blob(license_bytes)) != (spec['license_sha256'], spec['license_git_blob']):
        raise ValueError('license identity mismatch')
    expected_path = '/'.join(path.parts[:2]) + ('/OFL.txt' if path.parts[0] == 'ofl' else '/LICENSE.txt')
    if spec['license_path'] != expected_path: raise ValueError('license is not original family license')
    notice = license_bytes.decode('utf-8')
    identifier = spec['license_identifier']
    if identifier == 'OFL-1.1' and path.parts[0] == 'ofl':
        if 'SIL OPEN FONT LICENSE' not in notice: raise ValueError('unrecognized OFL notice')
        # Modified RFN fonts need an explicit reviewed renaming recipe, absent v1.
        header = notice.split('SIL OPEN FONT LICENSE', 1)[0].lower()
        if 'reserved font name' in header: raise ValueError('reserved font name requires reviewed renaming')
    elif identifier == 'Apache-2.0' and path.parts[0] == 'apache':
        if 'Apache License' not in notice or 'Version 2.0' not in notice: raise ValueError('unrecognized Apache notice')
    else: raise ValueError('redistribution license unsupported')
    if spec['weight'] not in (400, 700) or spec['style'] not in ('normal', 'italic'): raise ValueError('unsupported target face')

def axis_coordinates(font, requested):
    axes = font['fvar'].axes
    tags = [a.axisTag for a in axes]
    if len(tags) != len(set(tags)) or set(requested) != set(tags): raise ValueError('every source axis must be explicitly pinned')
    result = {}
    for axis in axes:
        coordinate = requested[axis.axisTag]
        if isinstance(coordinate, bool) or not isinstance(coordinate, (int, float)) or not math.isfinite(coordinate):
            raise ValueError('axis coordinate must be a finite scalar')
        if not axis.minValue <= coordinate <= axis.maxValue: raise ValueError('axis coordinate outside original range')
        result[axis.axisTag] = coordinate
    return dict(sorted(result.items()))

def derive(source, license_bytes, spec, tool_lock):
    verify_pins(source, license_bytes, spec)
    actual_tool = implementation_identity()
    if actual_tool != tool_lock: raise ValueError('instancer implementation identity mismatch')
    from fontTools.ttLib import TTFont
    from fontTools.varLib.instancer import instantiateVariableFont
    with TTFont(io.BytesIO(source), recalcTimestamp=False, lazy=False) as font:
        if 'fvar' not in font: raise ValueError('source is not variable')
        axes = axis_coordinates(font, spec['axes'])
        font['head'].created = font['head'].modified = EPOCH
        instance = instantiateVariableFont(font, axes, inplace=False, optimize=True, updateFontNames=False)
        try:
            instance.recalcTimestamp = False
            instance['head'].created = instance['head'].modified = EPOCH
            output = io.BytesIO()
            instance.save(output, reorderTables=True)
            data = output.getvalue()
        finally:
            instance.close()
    if len(data) > MAX_BYTES: raise ValueError('derived font envelope exceeded')
    # Independent existing static parser rejects residual variation, bitmap/color,
    # checksum and metadata incompatibilities. Runtime native admission still runs.
    if generate.face(data) != (spec['family'], spec['weight'], spec['style']):
        raise ValueError('derived static face does not match exact target')
    evidence = dict(schema_version=1, source=dict(spec), tool=actual_tool,
                    output_sha256=sha(data), output_size=len(data), license_notice=license_bytes.decode('utf-8'))
    evidence['derivation_id'] = 'font-derived_' + sha(canonical(evidence))
    return data, evidence
