"""Owned finite-workload process-tree RSS collector. No RAM limit or isolation.
Worker calls run_cycle(size). Parent measures OS RSS while it runs and at barriers;
workload stdout/stderr is captured as narration, never parsed as measurements.
"""
import contextlib
import importlib.util
import inspect
import json
import os
import pathlib
import platform
import selectors
import subprocess
import sys
import time

INTERVAL = 0.05
MAX_SAMPLES = 4096
# Liveness, not speed. /bin/ps reading a few hundred process-table rows answers
# in single-digit milliseconds on an idle machine, and takes longer — while
# still answering correctly — on a loaded one. The previous 0.15 s bound was a
# bound on this machine's speed: under a concurrent release build it turned a
# correct-but-slow sample into 'OS process measurement timed out', an incomplete
# measurement, and a check that read "unknown". 5 s is two orders of magnitude
# above any observed ps duration here and well inside the check's own timeout,
# so it is reached only when ps is not going to answer at all. A sample that is
# merely slow is now a measurement whose own duration is recorded (sample_ms).
SAMPLE_LIVENESS_SECONDS = 5.0

def worker(plan):
    # Private pipe protocol: workload print('success') cannot become a phase or verdict.
    fd = int(sys.argv[3])
    with os.fdopen(fd, 'w', buffering=1) as protocol, contextlib.redirect_stdout(sys.stderr):
        sys.path.insert(0, os.getcwd())
        spec=importlib.util.spec_from_file_location('thruline_workload',plan['module'])
        mod=importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
        run=mod.run_cycle
        if not callable(run) or inspect.iscoroutinefunction(run) or inspect.isgeneratorfunction(run) or inspect.isasyncgenfunction(run):
            raise RuntimeError("Unsupported workload: run_cycle must be a synchronous callable returning None")
        protocol.write('ready\n'); sys.stdin.readline()
        for i in range(plan['warmup_cycles']+plan['cycles']):
            if run(plan['size']) is not None:
                raise RuntimeError('Unsupported workload result; each completed cycle must return None')
            protocol.write(str(i)+'\n')
            if sys.stdin.readline().strip() != 'continue': return
        protocol.write('done\n'); sys.stdin.readline()

def snapshot(worker_pid, collector_pid, group):
    # ps has its own liveness bound and output bound. Communicate drains
    # concurrently. Returns the members and the sample's own duration in ms.
    began=time.monotonic()
    p=subprocess.Popen(['/bin/ps','-axo','pid=,ppid=,pgid=,rss='],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
    try:
        data=p.communicate(timeout=SAMPLE_LIVENESS_SECONDS)[0]
    except subprocess.TimeoutExpired:
        p.kill(); p.communicate(); raise RuntimeError('OS process measurement did not answer within its '+str(SAMPLE_LIVENESS_SECONDS)+'s liveness bound')
    sample_ms=round((time.monotonic()-began)*1000)
    if p.returncode or len(data)>1024*1024: raise RuntimeError('OS process table unavailable or over measurement limit')
    rows={}
    for line in data.splitlines():
        a=line.split()
        if len(a)!=4: raise RuntimeError('Unsupported process-table format')
        pid,ppid,pgid,kib=map(int,a); rows[pid]=(ppid,pgid,kib*1024)
    # Include surviving members of the launched group even if their parent exited.
    included={pid for pid,(_,pgid,_) in rows.items() if pgid==group and pid not in {collector_pid,p.pid}}
    included.add(worker_pid)
    for _ in range(128):
        new={pid for pid,(ppid,_,_) in rows.items() if ppid in included}-included
        if not new: break
        included.update(new)
    if len(included)>128: raise RuntimeError('Process tree exceeds 128 observed processes')
    members=[dict(pid=pid,ppid=rows[pid][0],rss_bytes=rows[pid][2]) for pid in sorted(included) if pid in rows]
    if worker_pid not in rows: raise RuntimeError('Workload exited before measurement barrier')
    return members,sample_ms

def measure(plan):
    start=time.monotonic()
    result=dict(protocol='thruline-memory/3',plan=plan,method='sampled process-tree RSS via /bin/ps; sum of resident pages at ~50ms plus phase barriers',
        environment=dict(os=dict(system=platform.system(),release=platform.release(),mac_version=platform.mac_ver()[0]),machine=platform.machine(),python=sys.version,executable=sys.executable),
        process_scope='Workload worker, observed descendants and surviving members of its process group. Collector and ps sampler excluded. Escaped/reparented descendants and between-sample peaks may be missed; shared pages may be counted more than once.',
        interval_ms=50,sample_liveness_bound_ms=round(SAMPLE_LIVENESS_SECONDS*1000),os_peak_rss_bytes=None,enforced_ram_limit_bytes=None,repetitions=[],issues=[],
        boundary='Finite accepted workload only. Sampled RSS is a lower bound on true peak; retained RSS includes allocator/cache behavior and is not by itself a leak. No indefinite bounded-memory claim, hostile-code isolation or hard RAM ceiling.')
    if os.getpgrp()!=os.getpid():
        result['issues'].append('Collector must run in its own controlled process group'); return result
    total_samples=0
    for rep in range(plan['repetitions']):
        begun=time.monotonic(); read_fd,write_fd=os.pipe()
        process=subprocess.Popen([sys.executable,'-I','-B',__file__,'worker',json.dumps(plan),str(write_fd)],stdin=subprocess.PIPE,pass_fds=(write_fd,),stdout=sys.stderr,stderr=sys.stderr)
        os.close(write_fd)
        row=dict(index=rep+1,worker_pid=process.pid,samples=[],phases=[],complete=False)
        result['repetitions'].append(row)
        selector=selectors.DefaultSelector(); selector.register(read_fd,selectors.EVENT_READ)
        pending=b''; phase='startup'; index=-1; members=[]
        try:
            while True:
                members,sample_ms=snapshot(process.pid,os.getpid(),os.getpgrp())
                total_samples+=1
                if total_samples>MAX_SAMPLES: raise RuntimeError('Measurement exceeds 4096 retained samples')
                row['samples'].append(dict(elapsed_ms=round((time.monotonic()-begun)*1000),sample_ms=sample_ms,phase=phase,rss_bytes=sum(m['rss_bytes'] for m in members),processes=members))
                if not selector.select(INTERVAL):
                    if process.poll() is not None: raise RuntimeError('Workload exited without all cycle barriers')
                    continue
                block=os.read(read_fd,1024)
                if not block: raise RuntimeError('Workload phase stream ended early')
                pending+=block
                if len(pending)>1024: raise RuntimeError('Invalid phase stream')
                if b'\n' not in pending: continue
                line,pending=pending.split(b'\n',1)
                if line==b'done':
                    if index!=plan['warmup_cycles']+plan['cycles']: raise RuntimeError('Missing completed cycles')
                    members,_=snapshot(process.pid,os.getpid(),os.getpgrp())
                    if any(m['pid'] != process.pid for m in members): raise RuntimeError('Descendants remained after the completed workload; process lifetime is incomplete')
                    process.stdin.write(b'continue\n'); process.stdin.flush()
                    # Liveness again, not speed: the workload has signalled 'done',
                    # so this waits for an exit that is already happening.
                    process.wait(timeout=SAMPLE_LIVENESS_SECONDS)
                    if process.returncode: raise RuntimeError('Workload did not exit successfully')
                    row['complete']=True; break
                expected=b'ready' if index==-1 else str(index).encode()
                # index tracks the next expected barrier, including startup.
                if line!=expected: raise RuntimeError('Unexpected workload phase marker')
                time.sleep(INTERVAL)
                members,sample_ms=snapshot(process.pid,os.getpid(),os.getpgrp())
                barrier=dict(cycle=index,kind='startup' if index==-1 else 'warmup' if index<plan['warmup_cycles'] else 'measured',sample_ms=sample_ms,rss_bytes=sum(m['rss_bytes'] for m in members),processes=members)
                row['phases'].append(barrier)
                total_samples+=1
                row['samples'].append(dict(elapsed_ms=round((time.monotonic()-begun)*1000),sample_ms=sample_ms,phase='barrier',rss_bytes=barrier['rss_bytes'],processes=members))
                process.stdin.write(b'continue\n'); process.stdin.flush()
                index+=1; phase='warmup' if index<plan['warmup_cycles'] else 'measured'
        except Exception as e:
            result['issues'].append('Repetition '+str(rep+1)+': '+str(e)[:500])
        finally:
            selector.close(); os.close(read_fd)
            # These are observed members of this check's newly created group.
            # A failed workload must not leave observed children running.
            for member in members:
                if member['pid'] != process.pid:
                    try: os.kill(member['pid'], 9)
                    except ProcessLookupError: pass
            if process.poll() is None: process.kill(); process.wait()
            process.stdin.close()
            row['duration_ms']=round((time.monotonic()-begun)*1000)
        if not row['complete']: break
    result['duration_ms']=round((time.monotonic()-start)*1000)
    return result

if __name__ == '__main__':
    if sys.argv[1]=='worker': worker(json.loads(sys.argv[2]))
    else:
        plan=json.loads(sys.argv[1])
        try: result=measure(plan)
        except Exception as e: result=dict(protocol='thruline-memory/3',plan=plan,repetitions=[],issues=['Collector incomplete: '+str(e)[:500]])
        print(json.dumps(result,sort_keys=True))
