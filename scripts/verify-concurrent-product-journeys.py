#!/usr/bin/env python3
"""Run independent real-media HTTP/CLI journeys under concurrent host load."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import time


ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/opencut')
parser.add_argument('--artifacts-dir', type=Path, required=True)
parser.add_argument('--parallel', type=int, default=6)
parser.add_argument('--runs', type=int, default=24,
                    help='independent product journeys; use at least 20 for a useful p95 sample')
args = parser.parse_args()
assert 1 <= args.parallel <= 16
assert args.runs >= 20
binary = args.binary.resolve()
evidence = args.artifacts_dir.resolve()
evidence.mkdir(parents=True, exist_ok=False)


def journey(index):
    path = evidence / str(index)
    started = time.monotonic()
    try:
        process = subprocess.run(
            ['python3', str(ROOT / 'scripts/verify-product-journey.py'),
             '--binary', str(binary), '--artifacts-dir', str(path)],
            cwd=ROOT, env=os.environ.copy(), text=True, capture_output=True, timeout=900,
        )
        code, stderr = process.returncode, process.stderr[-2000:]
    except subprocess.TimeoutExpired as error:
        code, stderr = 124, repr(error)
    latest = path / 'latest-journey.txt'
    report_path = Path(latest.read_text()) / 'report.json' if latest.exists() else None
    report = json.loads(report_path.read_text()) if report_path and report_path.exists() else {}
    trace_path = report_path.parent / 'trace.json' if report_path else None
    trace = json.loads(trace_path.read_text()) if trace_path and trace_path.exists() else []
    return {
        'index': index,
        'exit_code': code,
        'status': report.get('status'),
        'checks': len(report.get('checks', [])),
        'busy_responses': sum(
            entry['response'].get('error', {}).get('code') == 'DATABASE_BUSY'
            for entry in trace if entry['response'].get('error')
        ),
        'error': str(report.get('error') or stderr)[:2000],
        'evidence': str(report_path.parent) if report_path else None,
        'elapsed_seconds': round(time.monotonic() - started, 2),
    }


with ThreadPoolExecutor(max_workers=args.parallel) as pool:
    runs = list(pool.map(journey, range(args.runs)))
passed = all(
    item['exit_code'] == 0 and item['status'] == 'passed_with_remaining_scope'
    and item['checks'] == 19
    for item in runs
)


def percentile(values, fraction):
    ordered = sorted(values)
    rank = (len(ordered) - 1) * fraction
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return ordered[lower]
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (rank - lower)


elapsed = [item['elapsed_seconds'] for item in runs]
report = {
    'status': 'passed' if passed else 'failed',
    'binary': str(binary),
    'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
    'parallel': args.parallel,
    'sample_count': len(runs),
    'journey_elapsed_seconds': {
        'minimum': round(min(elapsed), 3),
        'mean': round(sum(elapsed) / len(elapsed), 3),
        'p50_linear_interpolated': round(percentile(elapsed, 0.50), 3),
        'p95_linear_interpolated': round(percentile(elapsed, 0.95), 3),
        'maximum': round(max(elapsed), 3),
    },
    'runs': runs,
    'busy_responses_total': sum(item['busy_responses'] for item in runs),
    'scope': 'Independent workspaces with real synthetic audio/video; does not prove full R0-R7 acceptance',
}
(evidence / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'status': report['status'], 'report': str(evidence / 'report.json'),
                  'sample_count': report['sample_count'],
                  'parallel': report['parallel'],
                  'journey_elapsed_seconds': report['journey_elapsed_seconds'],
                  'runs': [{key: item[key] for key in ('index', 'exit_code', 'status', 'checks', 'elapsed_seconds')}
                           for item in runs]}, indent=2))
raise SystemExit(0 if passed else 1)
