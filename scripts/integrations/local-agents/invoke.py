#!/usr/bin/env python3
"""Run native agents with local-only networking, scoped writes, and raw evidence.

This is a runtime bridge, not ThruLine product qualification. Success is decided
by an independent oracle outside the agent's writable directories.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import signal
import shutil
import subprocess
import time
import sys

# Runtime helpers are immutable evidence inputs, including across child runs.
sys.dont_write_bytecode = True

from model_identity import verify as verify_model
from runtime_identity import verify as verify_runtime
from process_ownership import Ownership

ROOT = Path(__file__).resolve().parents[3]
git_environment={'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':'/dev/null','GIT_CONFIG_SYSTEM':'/dev/null','GIT_TERMINAL_PROMPT':'0'}
git_prefix=['/usr/bin/git','-c','core.hooksPath=/dev/null','-c','core.fsmonitor=false','--no-pager']
p = argparse.ArgumentParser()
p.add_argument("repository", choices=["R12", "R13", "R14", "R15"])
p.add_argument("--workspace", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--task-file", type=Path, required=True)
p.add_argument("--model-provenance",type=Path,default=ROOT/'qualification/open-source-review/20260912-initial/local-agent-preparation/model-provenance.json')
p.add_argument("--container-runtime", type=Path)
p.add_argument("--feedback-manifest", type=Path)
p.add_argument("--aider-json")
p.add_argument("--openhands-json")
p.add_argument('--mini-json')
p.add_argument("--pr-agent-json", help="Explicit native plain-diff provider and patch file.")
p.add_argument("--resume-id")
p.add_argument("--conversation-id")
p.add_argument("--resume-from", type=Path)
p.add_argument("--interrupt-after-observations", type=int)
p.add_argument("--worker-owned-group", action="store_true", help="Inherit the product broker group so agent RSS and cancellation remain visible to its worker.")
p.add_argument("--local-model-port", type=int, choices=[11435, 11436], required=True, help="The one plan-bound loopback model destination.")
p.add_argument("--iterations", type=int, default=8)
p.add_argument("--command", default="review", choices=["review", "improve", "describe"])
p.add_argument("--files", nargs="+", default=["notes.py"])
a = p.parse_args()
if a.feedback_manifest and (a.repository != 'R12' or not a.container_runtime):
    raise SystemExit('native feedback requires Aider in the owned Linux runtime')
if a.worker_owned_group and os.getpgrp() != os.getppid():
    raise SystemExit('worker-owned-group requires the parent broker to lead this process group')
aider_mode = None
if a.aider_json:
    if a.repository != 'R12' or not a.container_runtime:
        raise SystemExit('native Aider modes require the owned Linux runtime')
    from aider_modes import validate as validate_aider_mode
    aider_mode = validate_aider_mode(json.loads(a.aider_json))
    if aider_mode['mode'] == 'ask' and a.feedback_manifest:
        raise SystemExit('Ask mode cannot run repair feedback')
openhands_options = None
if a.openhands_json:
    if a.repository != 'R13' or not a.container_runtime:
        raise SystemExit('OpenHands options require the owned Linux runtime')
    from openhands_options import validate as validate_openhands
    openhands_options = validate_openhands(json.loads(a.openhands_json))
pr_agent_options = None
if a.pr_agent_json:
    if a.repository != 'R14' or not a.container_runtime:
        raise SystemExit('native plain-diff requires PR-Agent in the owned Linux runtime')
    from pr_agent_options import validate as validate_pr_agent
    pr_agent_options = validate_pr_agent(json.loads(a.pr_agent_json), a.files)
mini_options = None
if a.mini_json:
    if a.repository != 'R15' or not a.container_runtime:
        raise SystemExit('mini-SWE harness requires the pinned Linux runtime')
    from mini_harness import validate as validate_mini
    mini_options = validate_mini(json.loads(a.mini_json))
workspace, out = a.workspace.resolve(), a.output.resolve()
out.mkdir(parents=True, exist_ok=True)
if (out / "invocation.json").exists():
    raise SystemExit("refusing to overwrite an earlier invocation")
if a.resume_from:
    if not a.resume_id or a.repository!='R13':
        raise SystemExit('resume-from requires R13 and an actual persisted conversation id')
    shutil.copytree(a.resume_from.resolve()/'persistence',out/'persistence')
runtime = ROOT / "qualification/runtimes/local-agents" / a.repository
python = runtime / "venv/bin/python"
task = a.task_file.resolve().read_bytes()
(out / "task.txt").write_bytes(task)
(out / "empty.env").touch()
(out / "empty.yml").write_text("{}\n")
(out / "temp").mkdir(exist_ok=True)
driver_dir=out/'driver-source'; driver_dir.mkdir()
driver_sources=[]
for path in sorted(Path(__file__).parent.glob('*.py')):
    data=path.read_bytes(); (driver_dir/path.name).write_bytes(data)
    driver_sources.append({'name':path.name,'source_path':str(path),'captured_path':str(driver_dir/path.name),'sha256':hashlib.sha256(data).hexdigest()})
account_home = Path(pwd.getpwuid(os.getuid()).pw_dir).resolve()
profile = ['(version 1)', '(allow default)', '(deny network*)',
           '(allow network-outbound (remote ip "localhost:'+str(a.local_model_port)+'"))',
           '(deny file-write*)', '(allow file-write* (literal "/dev/null"))',
           '(allow file-write* (literal "/dev/ptmx") (literal "/dev/tty") (regex #"^/dev/ttys[0-9]+$"))',
           '(deny file-read-data (subpath '+json.dumps(str(account_home))+'))']
for path in [runtime, workspace, out, Path(__file__).parent,
             account_home/'.local/share/uv/python']:
    profile.append('(allow file-read-data (subpath ' + json.dumps(str(path)) + '))')
profile.append('(allow file-write* (subpath ' + json.dumps(str(out)) + '))')
for name in a.files:
    selected=(workspace/name).resolve()
    if workspace not in selected.parents:
        raise SystemExit('selected writable source path escapes workspace')
    profile.append('(allow file-write* (literal '+json.dumps(str(selected))+'))')
    if a.repository=='R13':
        # Native FileEditor commits writes with tempfile.mkstemp in the source
        # directory. Authorize only that selected file's sibling temp pattern.
        temporary_pattern='^'+re.escape(str(selected.parent/('.'+selected.name+'.')))+'[a-z0-9_]+\\.tmp$'
        # SBPL #"..." regexes consume regex escapes directly. JSON escaping
        # doubles them and silently turns the allowance into a nonmatch.
        temporary_pattern=temporary_pattern.replace('"','\\"').replace('\n','\\n').replace('\r','\\r')
        profile.append('(allow file-write* (regex #"'+temporary_pattern+'"))')
profile.append('(deny file-write* (subpath '+json.dumps(str(workspace/'.git'))+'))')
profile.append('(deny file-write* (subpath '+json.dumps(str(driver_dir))+'))')
for name in ['sandbox.sb','task.txt','invocation.json','runtime-revalidation.json','model-revalidation.json','model-show-captured.json','model-tags-captured.json']:
    profile.append('(deny file-write* (literal '+json.dumps(str(out/name))+'))')
(out / "sandbox.sb").write_text("\n".join(profile) + "\n")
argv = ['/usr/bin/sandbox-exec', '-f', str(out / 'sandbox.sb'), str(python),
        str(driver_dir/'engine.py'), a.repository,
        '--workspace', str(workspace), '--output', str(out), '--task-file', str(out / 'task.txt'),
        '--iterations', str(a.iterations), '--command', a.command, '--files', *a.files]
if a.resume_id:
    argv += ['--resume-id', a.resume_id]
if a.conversation_id:
    argv += ['--conversation-id', a.conversation_id]
if a.interrupt_after_observations is not None:
    argv += ['--interrupt-after-observations', str(a.interrupt_after_observations)]
env = {'PATH': str(runtime / 'venv/bin') + ':/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin',
       'PYTHONDONTWRITEBYTECODE': '1', 'LITELLM_LOCAL_MODEL_COST_MAP': 'true',
       'LITELLM_TELEMETRY': 'false', 'DO_NOT_TRACK': '1', 'AIDER_ANALYTICS': 'false',
       'AIDER_CHECK_UPDATE': 'false', 'OLLAMA_API_BASE': 'http://127.0.0.1:'+str(a.local_model_port),
       'OPENAI_API_BASE': 'http://127.0.0.1:'+str(a.local_model_port)+'/v1', 'OPENAI_API_KEY': 'local-no-secret',
       'TMPDIR': str(out / 'temp'), 'XDG_CACHE_HOME': str(out / 'cache'),
       'XDG_CONFIG_HOME': str(out / 'config'), 'MSWEA_SILENT_STARTUP': '1',
       'HF_HUB_OFFLINE': '1', 'TRANSFORMERS_OFFLINE': '1', 'TERM': 'dumb',
       'OTEL_SDK_DISABLED': 'true', 'LOG_LEVEL': 'INFO'}
env['AZURE_DEVOPS_CACHE_DIR'] = str(out / 'cache/azure-devops')
env['OH_PERSISTENCE_DIR'] = str(out / 'openhands-state')
env['PYTHONPYCACHEPREFIX'] = str(out / 'unused-bytecode-cache')
env.update({key:value for key,value in git_environment.items() if key!='PATH'})
if a.repository == 'R12':
    env['PYTHONPATH'] = str(runtime / 'source')
record = {'schema': 'thruline.local-agent-invocation.v1', 'repository_id': a.repository,
          'driver_sources':driver_sources,
          'argv': argv, 'environment': env, 'started_at': datetime.datetime.now(datetime.UTC).isoformat(),
          'task_sha256': hashlib.sha256(task).hexdigest(), 'qualification_status': 'runtime_exercise_only',
          'workspace_before': subprocess.check_output([*git_prefix, '-C', str(workspace), 'diff','--no-ext-diff','--no-textconv', 'HEAD'],env=git_environment, text=True),
          'head': subprocess.check_output([*git_prefix, '-C', str(workspace), 'rev-parse', 'HEAD'],env=git_environment, text=True).strip(),
          'installation': None if a.container_runtime else json.loads((runtime / 'installation.json').read_text())}
model_provenance = a.model_provenance.resolve()
if model_provenance.is_file():
    record['model_provenance'] = {'path':str(model_provenance), 'sha256':hashlib.sha256(model_provenance.read_bytes()).hexdigest()}
try:
    if a.container_runtime:
        from container_native import runtime as container_runtime
        container_config, _, container_image = container_runtime(a.container_runtime)
        if container_config['repository_id'] != a.repository:
            raise ValueError('Linux image belongs to another native provider')
        record['installation'] = {'repository_id': a.repository, 'source_commit': container_config['source_commit'], 'container_runtime': str(a.container_runtime), 'image': container_image['Id']}
        record['runtime_revalidation'] = {'backend': 'pinned-linux-image', 'matched': True, 'manifest_sha256': hashlib.sha256(a.container_runtime.read_bytes()).hexdigest(), 'image': container_image['Id']}
        (out/'runtime-revalidation.json').write_text(json.dumps(record['runtime_revalidation'],indent=2)+'\n')
    else:
        record['runtime_revalidation'] = verify_runtime(runtime, out)
    record['model_revalidation'] = verify_model(model_provenance, out, a.local_model_port)
    feedback_directory = None
    if a.feedback_manifest:
        from feedback import read_regular, freeze as freeze_feedback
        data = read_regular(a.feedback_manifest, 32768)
        feedback_directory = out/'feedback-input'
        record['native_feedback'] = freeze_feedback(json.loads(data), workspace, feedback_directory, a.files)
        (out/'feedback-manifest.json').write_bytes(data)
    selected_model=json.loads(model_provenance.read_text())['model']
    argv += ['--model-name',selected_model]
except Exception as error:
    record.update(exit_code=4, state='preflight_failed', elapsed_seconds=0,
                  finished_at=datetime.datetime.now(datetime.UTC).isoformat(), error=str(error))
    (out/'invocation.json').write_text(json.dumps(record,indent=2)+'\n')
    raise SystemExit('runtime/model identity preflight failed: '+str(error))
started = time.monotonic()
if a.container_runtime:
    from container_native import run_native
    try:
        code, state, group_cleanup = run_native(a.container_runtime, workspace, out, selected_model, a.local_model_port, out/'task.txt', a.files, a.iterations, a.repository, a.command, a.resume_id, a.interrupt_after_observations, a.conversation_id, feedback_directory, aider_mode, openhands_options, pr_agent_options, mini_options)
    except Exception as error:
        code, state = 4, 'capture_failed'
        record['error'] = str(error)
        group_cleanup = {'observed_owned_processes_terminated': False, 'termination_scope': 'Outer product worker must establish namespace termination after broker failure'}
    if (out/'container-command.json').is_file():
        record['argv'] = json.loads((out/'container-command.json').read_text())['argv']
    record['environment'] = {'native': 'explicit environment in captured container argv; no host credentials or IPC', 'transport': 'plan-bound loopback filesystem mailbox on port '+str(a.local_model_port)}
else:
    with (out / 'stdout.log').open('wb') as stdout, (out / 'stderr.log').open('wb') as stderr:
        process = subprocess.Popen(argv, cwd=workspace, env=env, stdout=stdout, stderr=stderr, start_new_session=not a.worker_owned_group)
        ownership = Ownership(process.pid, product_group=a.worker_owned_group)
        interrupted = False
        def request_stop(_signum, _frame):
            global interrupted
            interrupted = True
        signal.signal(signal.SIGTERM, request_stop)
        signal.signal(signal.SIGINT, request_stop)
        state = 'exited'
        while process.poll() is None:
            try:
                ownership.sample()
            except (OSError, RuntimeError, subprocess.SubprocessError) as error:
                ownership.failures.append(str(error))
                state='ownership_unknown'
                break
            size = sum(path.stat().st_size for path in out.rglob('*') if path.is_file())
            if interrupted or time.monotonic() - started > 600 or size > 16 * 1024 * 1024:
                state = 'interrupted' if interrupted else 'timed_out' if time.monotonic() - started > 600 else 'output_limit'
                break
            time.sleep(0.2)
        cleanup = ownership.terminate(process)
        if process.poll() is None:
            process.kill()
        try:
            code = process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            code = 5
            cleanup['observed_owned_processes_terminated'] = False
            cleanup['failures'].append('native leader could not be reaped within 3 seconds')
        # An agent may leave ordinary background terminal children in its session.
        # Stop that owned group before the broker observes source/output bytes.
        if not a.worker_owned_group:
            try:
                os.killpg(process.pid,signal.SIGKILL)
            except ProcessLookupError:
                pass
        group_cleanup=cleanup
        if not cleanup['observed_owned_processes_terminated']:
            state='termination_unestablished'
        if code == 0 and state != 'exited':
            code=5
record.update(exit_code=code, state=state, elapsed_seconds=time.monotonic()-started,
              process_group_cleanup=group_cleanup, worker_owned_group=a.worker_owned_group,
              finished_at=datetime.datetime.now(datetime.UTC).isoformat(),
              workspace_after=subprocess.check_output([*git_prefix, '-C', str(workspace), 'diff','--no-ext-diff','--no-textconv', 'HEAD'],env=git_environment, text=True))
(out / 'invocation.json').write_text(json.dumps(record, indent=2) + '\n')
print(json.dumps({'repository_id': a.repository, 'exit_code': code, 'state': state, 'output': str(out)}))
raise SystemExit(code if code >= 0 else 1)
