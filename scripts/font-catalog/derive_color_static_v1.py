#!/usr/bin/env python3
"""Fully instantiate COLRv1 after the frozen V2 outline/name recipe.

V2 inputs, algorithm, proofs and bytes stay unchanged. Every source axis is
pinned; original fvar/avar normalization selects color deltas before all variable
paint/transform/stop/clip constructs are converted to their static counterparts.
"""
import copy
import io
import math
from pathlib import Path
import derive_renamed_v2 as base

PROFILE = 'opencut-static-colr-opaque-names-v1'
PROVIDER = 'opencut-fonttools-static-colr-v1'
VARIABLE_PAINTS = {3, 5, 7, 9, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31}
VARIABLE_CLASSES = {'VarColorLine': 'ColorLine', 'VarColorStop': 'ColorStop',
                    'VarAffine2x3': 'Affine2x3'}
canonical = base.canonical
sha = base.sha


def implementation_identity():
    identity = base.implementation_identity()
    identity['base_derivation_sha256'] = identity['derivation_sha256']
    identity['derivation_sha256'] = sha(Path(__file__).read_bytes())
    return identity


def target_names(base_source, tool):
    token = sha(canonical(dict(profile=PROFILE,
        base_derivation_id=base_source['face']['derivation_id'],
        color_algorithm_sha256=tool['derivation_sha256'])))[:24]
    return 'OCDF COLR ' + token, 'OCDF-COLR-' + token


def freeze_colr(original, instance, coordinates):
    from fontTools.misc.roundTools import otRound
    from fontTools.ttLib.tables import otTables as ot
    from fontTools.ttLib.tables.otBase import BaseTable
    from fontTools.varLib.instancer import AxisLimits
    from fontTools.varLib.varStore import VarStoreInstancer, NO_VARIATION_INDEX
    if 'fvar' not in original or 'fvar' in instance:
        raise ValueError('original variable and frozen V2 outline faces required')
    base.v1.axis_coordinates(original, coordinates)
    limits = AxisLimits(coordinates).limitAxesAndPopulateDefaults(original)
    location = limits.normalize(original).pinnedLocation()
    if set(location) != set(coordinates):
        raise ValueError('color instantiation must pin every normalized source axis')
    colr = instance['COLR']
    if colr.version != 1 or 'CPAL' not in instance:
        raise ValueError('only a fixed COLRv1 color source is supported')
    table = colr.table
    store, mapping = table.VarStore, table.VarIndexMap
    if store is None or store.VarRegionList.RegionAxisCount != len(original['fvar'].axes):
        raise ValueError('COLR variable store differs from original source axes')
    instancer = VarStoreInstancer(store, original['fvar'].axes, location)
    visited = set()
    counts = dict(variable_tables=0, variable_fields=0, changed_fields=0,
                  clamped_alpha_fields=0, sorted_color_lines=0)

    def walk(value, depth=0):
        if isinstance(value, (str, bytes, int, float, type(None))):
            return
        if id(value) in visited:
            return
        if depth >= 64 or len(visited) >= 1_000_000:
            raise ValueError('color graph exceeds its fixed depth/count envelope')
        visited.add(id(value))
        if isinstance(value, dict):
            children = list(value.values())
        elif isinstance(value, (list, tuple)):
            children = list(value)
        elif isinstance(value, BaseTable):
            children = [child for key, child in vars(value).items()
                        if key not in ('VarStore', 'VarIndexMap')]
        else:
            raise ValueError('unknown color table object')
        for child in children:
            walk(child, depth + 1)
        if not isinstance(value, BaseTable):
            return
        attrs = value.getVariableAttrs()
        if attrs:
            counts['variable_tables'] += 1
            first = value.VarIndexBase
            for attr in attrs:
                converter = value.getConverterByName(attr)
                offset = converter.getVarIndexOffset()
                index = NO_VARIATION_INDEX if first == NO_VARIATION_INDEX else first + offset
                if not 0 <= index <= NO_VARIATION_INDEX:
                    raise ValueError('color variation index overflow')
                mapped = mapping[index] if mapping is not None else index
                delta = instancer[mapped]
                current = getattr(value, attr)
                if not math.isfinite(delta) or not math.isfinite(current):
                    raise ValueError('nonfinite color instance')
                fixed = hasattr(converter, 'toInt') and hasattr(converter, 'fromInt')
                encoded = converter.toInt(current) if fixed else current
                adjusted = otRound(encoded + delta)
                # OpenType clamps the varied alpha, rather than serializing an
                # out-of-range value into a supposedly static paint record.
                if attr == 'Alpha':
                    if getattr(converter, 'precisionBits', None) != 14:
                        raise ValueError('unexpected alpha representation')
                    clamped = min(16384, max(0, adjusted))
                    counts['clamped_alpha_fields'] += clamped != adjusted
                    adjusted = clamped
                changed = converter.fromInt(adjusted) if fixed else adjusted
                setattr(value, attr, changed)
                counts['variable_fields'] += 1
                counts['changed_fields'] += changed != current
            del value.VarIndexBase
        name = type(value).__name__
        if name == 'Paint' and value.Format in VARIABLE_PAINTS:
            value.Format -= 1
        elif name == 'ClipBox' and value.Format == 2:
            value.Format = 1
        elif name in VARIABLE_CLASSES:
            value.__class__ = getattr(ot, VARIABLE_CLASSES[name])
        if type(value).__name__ == 'ColorLine':
            ordered = sorted(value.ColorStop, key=lambda stop: stop.StopOffset)
            counts['sorted_color_lines'] += ordered != value.ColorStop
            value.ColorStop = ordered
        if value.getVariableAttrs() or hasattr(value, 'VarIndexBase'):
            raise ValueError('unconverted variable color table')

    walk(table)
    if not counts['variable_tables']:
        raise ValueError('recipe does not contain variable COLR fields')
    table.VarStore = None
    table.VarIndexMap = None
    return dict(profile='opencut-colr-complete-static-v1',
                normalized_location=dict(sorted(location.items())),
                removed_var_store=True, removed_var_index_map=True, **counts)


def derive(source, notice, base_source, tool_lock, base_output=None):
    from fontTools.ttLib import TTFont
    actual_tool = implementation_identity()
    if actual_tool != tool_lock:
        raise ValueError('static color algorithm/runtime identity mismatch')
    base_proof = __import__('json').loads(base_source['proof_json'])
    recipe = base_proof['source']
    base.verify_pins(source, notice, recipe)
    if base_proof['schema_version'] != 2 or recipe['render_profile'] != 'colr_v1':
        raise ValueError('fixed original V2 color recipe required')
    if base_output is None:
        base_output, produced = base.derive(source, notice, recipe, base_proof['tool'])
        if canonical(produced) != canonical(base_proof):
            raise ValueError('frozen base derivation differs from original proof')
    if (sha(base_output), len(base_output)) != (
            base_source['face']['content_hash'], int(base_source['face']['size_bytes'])):
        raise ValueError('original frozen output identity differs')
    family, postscript = target_names(base_source, actual_tool)
    with TTFont(io.BytesIO(source), recalcTimestamp=False, lazy=False) as original, \
         TTFont(io.BytesIO(base_output), recalcTimestamp=False, lazy=False) as instance:
        protected = {tag: sha(instance.reader[tag]) for tag in instance.reader.keys()
                     if tag not in ('head', 'name', 'COLR')}
        color = freeze_colr(original, instance, recipe['axes'])
        names_recipe = dict(recipe, family=family, output_postscript_name=postscript,
                            name_rewrite_version=PROFILE)
        names = base.rewrite_names(instance, names_recipe)
        instance['head'].created = instance['head'].modified = base.v1.EPOCH
        output = io.BytesIO()
        instance.save(output, reorderTables=True)
        data = base.reassemble_static(base_output, output.getvalue(), {'head', 'name', 'COLR'})
    if not 0 < len(data) <= base.MAX_BYTES:
        raise ValueError('static color output exceeds its original bounded envelope')
    with TTFont(io.BytesIO(data), recalcTimestamp=False, lazy=False) as finished:
        if protected != {tag: sha(finished.reader[tag]) for tag in protected}:
            raise ValueError('static color conversion changed frozen outline/metric tables')
        if (finished['name'].getBestFamilyName(), finished['OS/2'].usWeightClass) != (
                family, base_source['face']['weight']):
            raise ValueError('static color native identity mismatch')
        if finished['COLR'].table.VarStore or finished['COLR'].table.VarIndexMap:
            raise ValueError('variable color storage survived serialization')
    spec = dict(names_recipe, recipe_kind='instantiate_static_colr_v1')
    proof = dict(schema_version=4, kind='complete_static_colr', source=spec,
                 base_source=copy.deepcopy(base_source), tool=actual_tool,
                 output_sha256=sha(data), output_size=len(data),
                 license_notice=notice.decode('utf-8'), name_rewrite=names,
                 color_instantiation=color, preserved_base_tables=protected)
    proof['derivation_id'] = 'font-color-static-v1_' + sha(canonical(proof))
    return data, proof
