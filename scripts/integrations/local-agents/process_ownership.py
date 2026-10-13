"""Observe owned native descendants across SDK-created process groups.

This is sampled process supervision, not PID-namespace containment. A process
that double-forks and is reparented between samples may escape discovery; report
that limit rather than asserting universal termination.
"""
import os
import signal
import subprocess
import time


def snapshot():
    process = subprocess.Popen(['/bin/ps', '-axo', 'pid=,ppid=,pgid=,rss=,lstart='],
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                               env={'PATH': '/usr/bin:/bin'})
    stdout, _ = process.communicate(timeout=5)
    if process.returncode:
        raise RuntimeError('cannot observe native process ownership')
    rows = {}
    for line in stdout.splitlines():
        parts = line.split(None, 4)
        if len(parts) != 5:
            raise RuntimeError('unrecognized process ownership row')
        pid, ppid, pgid, rss = map(int, parts[:4])
        if pid == process.pid:
            continue
        rows[pid] = {'pid': pid, 'ppid': ppid, 'pgid': pgid, 'rss_bytes': rss * 1024, 'started': parts[4]}
    return rows


class Ownership:
    def __init__(self, root_pid, product_group=False):
        self.root_pid = root_pid
        self.product_group = os.getpgrp() if product_group else None
        self.exclude = {os.getpid(), os.getppid()}
        self.known = {}
        self.peak_rss_bytes = 0
        self.samples = 0
        self.failures = []

    def sample(self):
        rows = snapshot()
        roots = {pid for pid, identity in self.known.items() if pid in rows and rows[pid]['started'] == identity['started']}
        if self.root_pid in rows and self.root_pid not in self.known:
            roots.add(self.root_pid)
        if self.product_group:
            roots.update(pid for pid, row in rows.items() if row['pgid'] == self.product_group and pid not in self.exclude)
        changed = True
        while changed:
            added = {pid for pid, row in rows.items() if row['ppid'] in roots} - roots - self.exclude
            changed = bool(added)
            roots.update(added)
        current = {}
        for pid in roots:
            row = rows[pid]
            if pid in self.known and self.known[pid]['started'] != row['started']:
                continue
            self.known[pid] = row
            current[pid] = row
        self.peak_rss_bytes = max(self.peak_rss_bytes, sum(row['rss_bytes'] for row in current.values()))
        self.samples += 1
        return current

    def terminate(self, process, seconds=6):
        deadline = time.monotonic() + seconds
        remaining = {}
        while time.monotonic() < deadline:
            process.poll()
            try:
                remaining = self.sample()
            except (OSError, RuntimeError, subprocess.SubprocessError) as error:
                self.failures.append(str(error))
                break
            if not remaining:
                break
            # Kill children first while ancestry is observable; no broad process
            # names, unrelated groups, or recycled PID are targeted.
            for pid in sorted(remaining, key=lambda value: value == self.root_pid):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            time.sleep(.05)
        process.poll()
        try:
            remaining = self.sample()
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            self.failures.append(str(error))
        return {'observed_owned_processes_terminated': not remaining and not self.failures,
                'remaining_observed_pids': sorted(remaining), 'failures': self.failures,
                'observed_processes': list(self.known.values()), 'samples': self.samples,
                'sampled_peak_rss_bytes': self.peak_rss_bytes,
                'product_group': self.product_group,
                'limitation': 'Sampled PPID/process-group ownership; a fast double-fork/reparent between samples is not guaranteed to be discovered. No PID-namespace containment claim.'}
