"""Observable, durable command stages for package acceptance."""

import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time


def write_json(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + '.tmp')
    with temporary.open('w') as output:
        json.dump(value, output, indent=2)
        output.write('\n')
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)


class AcceptanceRunner:
    def __init__(self, evidence, heartbeat_seconds=30):
        self.evidence = Path(evidence)
        (self.evidence / 'stage-logs').mkdir(exist_ok=True)
        self.commands = []
        self.stages = []
        self.started = time.monotonic()
        self.heartbeat_seconds = heartbeat_seconds
        self._persist()

    def _persist(self):
        write_json(self.evidence / 'stages.json', self.stages)

    def flush_commands(self):
        write_json(self.evidence / 'commands.json', self.commands)

    def _announce(self, message):
        print(f'[acceptance +{time.monotonic() - self.started:.1f}s] {message}',
              file=sys.stderr, flush=True)

    def checkpoint(self, name, **details):
        stage = {'name': name, 'status': 'passed', 'elapsed_seconds': 0, **details}
        self.stages.append(stage)
        self._persist()
        self._announce(f'PASS {name}')

    def failure(self, error):
        self.stages.append({'name': 'acceptance', 'status': 'failed',
                            'elapsed_seconds': round(time.monotonic() - self.started, 3),
                            'error': repr(error)})
        self._persist()
        self._announce(f'FAIL acceptance: {error!r}; evidence: {self.evidence}')

    def run(self, args, cwd=None, env=None, timeout=600, name=None):
        command = [str(x) for x in args]
        cwd = Path(cwd) if cwd is not None else self.evidence
        name = name or Path(command[1] if command[0].endswith('python3') and len(command) > 1 else command[0]).name
        stage = {'name': name, 'status': 'running', 'command': command, 'cwd': str(cwd)}
        self.stages.append(stage)
        self._persist()
        self._announce(f'START {name}')
        started = time.monotonic()
        done = threading.Event()
        def heartbeat():
            while not done.wait(self.heartbeat_seconds):
                self._announce(f'RUNNING {name} ({time.monotonic() - started:.1f}s)')
        ticker = threading.Thread(target=heartbeat, daemon=True)
        ticker.start()
        try:
            value = subprocess.run(command, cwd=cwd, env=env, text=True,
                                   capture_output=True, timeout=timeout)
        except Exception as error:
            def as_text(value):
                return value.decode(errors='replace') if isinstance(value, bytes) else value
            record = {'command': command, 'cwd': str(cwd), 'exit_code': None,
                      'stdout': as_text(getattr(error, 'stdout', None)),
                      'stderr': as_text(getattr(error, 'stderr', None)),
                      'error': repr(error)}
            self.commands.append(record)
            log_path = self.evidence / 'stage-logs' / f'{len(self.stages):03d}.json'
            write_json(log_path, record)
            stage.update(status='failed', elapsed_seconds=round(time.monotonic() - started, 3),
                         error=repr(error), log=str(log_path))
            self._persist()
            self._announce(f'FAIL {name} ({stage["elapsed_seconds"]:.1f}s)')
            raise
        finally:
            done.set()
            ticker.join(timeout=1)
        record = {'command': command, 'cwd': str(cwd), 'exit_code': value.returncode,
                  'stdout': value.stdout, 'stderr': value.stderr}
        self.commands.append(record)
        log_path = self.evidence / 'stage-logs' / f'{len(self.stages):03d}.json'
        write_json(log_path, record)
        stage.update(status='passed' if value.returncode == 0 else 'failed',
                     elapsed_seconds=round(time.monotonic() - started, 3),
                     exit_code=value.returncode, log=str(log_path))
        self._persist()
        outcome = 'PASS' if value.returncode == 0 else 'FAIL'
        self._announce(f'{outcome} {name} ({stage["elapsed_seconds"]:.1f}s)')
        assert value.returncode == 0, (command, value.stdout, value.stderr)
        return value.stdout
