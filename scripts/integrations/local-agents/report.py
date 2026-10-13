#!/usr/bin/env python3
"""Capture a real, isolated native local-agent run for a product worker.

Selected file scope is explicit. This script never applies proposals to the
original checkout and never classifies agent submission as accepted behavior.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import pwd
import stat
import subprocess
import sys
import uuid

# Runtime helpers are immutable evidence inputs, including across child runs.
sys.dont_write_bytecode = True

ROOT=Path(__file__).resolve().parents[3]
p=argparse.ArgumentParser()
p.add_argument('--agent',required=True,choices=['R12','R13','R14','R15'])
p.add_argument('--source',type=Path,default=Path.cwd())
p.add_argument('--output-base',type=Path,required=True)
p.add_argument('--task-file',type=Path,required=True)
p.add_argument('--model-provenance',type=Path,help='Explicit prepared local model manifest; defaults to the pinned Qwen2.5 Coder 7B profile.')
p.add_argument('--files',nargs='+',required=True)
p.add_argument('--edit-files',nargs='+',help='Optional editable subset of captured files; the remaining captured files provide read-only repository context.')
p.add_argument('--interrupt-after-observations',type=int,help='Request the real OpenHands SDK interrupt API after this many native observations; the resulting paused attempt remains incomplete.')
p.add_argument('--conversation-id',help='Operator-bound UUID for a fresh OpenHands conversation')
p.add_argument('--openhands-json',help='Accepted native condensation and owned SDK/server transport options.')
p.add_argument('--mini-json')
p.add_argument('--aider-json',help='Accepted native Aider mode, edit format and reflection bound.')
p.add_argument("--pr-agent-json", help="Explicit native plain-diff provider and patch file.")
p.add_argument('--feedback-json',help='Accepted Aider development-feedback configuration; selected test bytes are frozen read-only.')
p.add_argument('--base-ref',help='Required for R14; a local Git ref used for the native PR diff.')
p.add_argument('--iterations',type=int,default=8)
p.add_argument('--command',choices=['review','improve','describe'],default='review')
p.add_argument('--oracle-config',type=Path,help='Frozen native oracle argv and bound file hashes; executed by the supervisor before and after the proposal.')
p.add_argument('--container-runtime',type=Path)
p.add_argument('--worker-owned-group',action='store_true',help='Keep native agent in the product worker process group for cancellation and RSS accounting.')
p.add_argument('--local-model-port',type=int,choices=[11435,11436],required=True,help='The one plan-bound loopback model destination.')
a=p.parse_args()
if a.feedback_json and (a.agent != 'R12' or not a.container_runtime):
    raise SystemExit('native feedback requires Aider in the owned Linux runtime')
aider_mode = None
if a.aider_json:
    if a.agent != 'R12' or not a.container_runtime:
        raise SystemExit('native Aider modes require the owned Linux runtime')
    from aider_modes import validate as validate_aider_mode
    aider_mode = validate_aider_mode(json.loads(a.aider_json))
    if aider_mode['mode'] == 'ask' and (a.edit_files is not None or a.feedback_json):
        raise SystemExit('Ask mode captures read-only context and cannot accept edits or repair feedback')
openhands_options = None
if a.openhands_json:
    if a.agent != 'R13' or not a.container_runtime:
        raise SystemExit('OpenHands options require the owned Linux runtime')
    from openhands_options import validate as validate_openhands
    openhands_options = validate_openhands(json.loads(a.openhands_json))
pr_agent_options = None
if a.pr_agent_json:
    if a.agent != 'R14' or not a.container_runtime or a.base_ref or a.edit_files is not None:
        raise SystemExit('plain-diff requires the owned PR-Agent runtime and cannot select committed history or editable context')
    from pr_agent_options import validate as validate_pr_agent, read_input as read_pr_agent_input
    pr_agent_options = validate_pr_agent(json.loads(a.pr_agent_json), a.files)
mini_options = None
if a.mini_json:
    if a.agent != 'R15' or not a.container_runtime:
        raise SystemExit('mini-SWE harness requires the pinned Linux runtime')
    from mini_harness import validate as validate_mini
    mini_options = validate_mini(json.loads(a.mini_json))
source=a.source.resolve(); output_base=a.output_base.resolve()
if pr_agent_options:
    read_pr_agent_input(source, pr_agent_options)
git_environment={'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','GIT_CONFIG_NOSYSTEM':'1','GIT_CONFIG_GLOBAL':'/dev/null','GIT_CONFIG_SYSTEM':'/dev/null','GIT_TERMINAL_PROMPT':'0','GIT_OPTIONAL_LOCKS':'0'}
git_prefix=['/usr/bin/git','-c','core.hooksPath=/dev/null','-c','core.fsmonitor=false','--no-pager']
def source_git(*args):
    return subprocess.check_output([*git_prefix,'-C',str(source),*args],env=git_environment)
if output_base==source or source in output_base.parents:
    raise SystemExit('output-base must be outside the analyzed source tree')
if not 1<=a.iterations<=20 or not 1<=len(a.files)<=128:
    raise SystemExit('iteration/file budget exceeded')
head=source_git('rev-parse','HEAD').decode().strip()
base=None
if a.agent=='R14' and not pr_agent_options:
    if not a.base_ref or a.base_ref.startswith('-'):
        raise SystemExit('R14 requires a safe explicit base-ref')
    base_ref=source_git('rev-parse','--verify',a.base_ref+'^{commit}').decode().strip()
    base=source_git('merge-base',head,base_ref).decode().strip()
files=[]
for name in a.files:
    path=Path(name)
    if path.is_absolute() or '..' in path.parts or '.git' in path.parts:
        raise SystemExit('unsafe selected path')
    if (a.agent!='R14' and not (source/path).is_file()) or (source/path).is_symlink() or source not in (source/path).resolve().parents:
        raise SystemExit('selected path is not a contained regular file')
    files.append(path.as_posix())
files=sorted(set(files))
editable=sorted(set(a.edit_files)) if a.edit_files is not None else files
if not editable or not set(editable)<=set(files) or (a.agent=='R14' and a.edit_files is not None):
    raise SystemExit('editable scope must be a nonempty subset of captured files; R14 reviews its committed scope')
if sum((source/name).stat().st_size for name in files if (source/name).is_file())>8*1024*1024:
    raise SystemExit('source context exceeds 8 MiB')
run=output_base/str(uuid.uuid4()); workspace=run/'workspace'; workspace.mkdir(parents=True)
def sha(data): return hashlib.sha256(data).hexdigest()
def safe_artifact_bytes(path,root,limit=16*1024*1024):
    """Untrusted agent artifacts must never redirect privileged broker reads."""
    relative=path.relative_to(root)
    current=root
    for part in relative.parts:
        current=current/part
        if current.is_symlink():
            raise ValueError('symlink rejected at artifact boundary: '+str(current))
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError('nonregular artifact rejected: '+str(path))
    descriptor=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    with os.fdopen(descriptor,'rb') as stream:
        metadata=os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size>limit:
            raise ValueError('nonregular or oversized artifact rejected: '+str(path))
        data=stream.read(limit+1)
    if len(data)>limit:raise ValueError('artifact grew beyond capture limit: '+str(path))
    return data
def git(*args):
    return subprocess.check_output([*git_prefix,'-C',str(workspace),*args],env=git_environment,stderr=subprocess.STDOUT)
def inventory():
    return [{'path':name,'exists':(workspace/name).is_file(),'sha256':sha((workspace/name).read_bytes()) if (workspace/name).is_file() else None,'size':(workspace/name).stat().st_size if (workspace/name).is_file() else 0} for name in files]
def commit(message):
    git('add','--all','--',*files)
    git('-c','user.name=ThruLine isolated proposal','-c','user.email=qualification@localhost','commit','--allow-empty','-m',message)
git('init','-b','main')
git('config','--local','user.name','ThruLine isolated proposal')
git('config','--local','user.email','qualification@localhost')
git('config','--local','core.hooksPath','/dev/null')
git('config','--local','core.fsmonitor','false')
original={name:(source/name).read_bytes() if (source/name).is_file() else None for name in files}
captured=dict(original)
dirty_paths=source_git('diff','--no-ext-diff','--no-textconv','HEAD','--name-only','-z').decode().split('\0')
untracked_paths=source_git('ls-files','--others','--exclude-standard','-z').decode().split('\0')
if a.agent=='R14' and not pr_agent_options:
    for name in files:
        before=subprocess.run([*git_prefix,'-C',str(source),'show',base+':'+name],env=git_environment,capture_output=True)
        current=subprocess.run([*git_prefix,'-C',str(source),'show',head+':'+name],env=git_environment,capture_output=True)
        if before.returncode!=0 and current.returncode!=0:
            raise SystemExit('selected file absent from both committed revisions: '+name)
        captured[name]=current.stdout if current.returncode==0 else None
        if before.returncode==0:
            (workspace/name).parent.mkdir(parents=True,exist_ok=True)
            (workspace/name).write_bytes(before.stdout)
    git('add','--all')
    git('-c','user.name=ThruLine isolated proposal','-c','user.email=qualification@localhost','commit','--allow-empty','-m','Selected real base revision')
    git('checkout','-b','review-change')
if sum(len(data) for data in captured.values() if data is not None)>8*1024*1024:
    raise SystemExit('captured context exceeds 8 MiB')
for name,data in captured.items():
    (workspace/name).parent.mkdir(parents=True,exist_ok=True)
    if data is None:
        (workspace/name).unlink(missing_ok=True)
    else:
        (workspace/name).write_bytes(data)
commit('Selected captured source')
before=inventory()
task=a.task_file.read_bytes()
if len(task)>32768:
    raise SystemExit('task exceeds 32 KiB')
(run/'task.txt').write_bytes(task)
oracle=None
if a.oracle_config and not pr_agent_options:
    oracle=json.loads(a.oracle_config.read_text())
    if oracle.get('schema')!='thruline.oracle-command.v1' or not oracle.get('bound_files') or not isinstance(oracle.get('argv'),list):
        raise SystemExit('oracle requires argv and frozen bound_files')
    (run/'oracle-config.json').write_bytes(a.oracle_config.read_bytes())
def run_oracle(label):
    if oracle is None:return None
    for binding in oracle['bound_files']:
        if sha(Path(binding['path']).read_bytes())!=binding['sha256']:
            raise SystemExit('frozen oracle input digest mismatch')
    argv=[str(part).replace('{workspace}',str(workspace)) for part in oracle['argv']]
    if not a.container_runtime and Path(argv[0]).name in ('python','python3','python3.12','python3.13'):
        argv[1:1]=['-B','-X','pycache_prefix=/dev/null']
    oracle_output=run/label; oracle_output.mkdir()
    account_home=Path(pwd.getpwuid(os.getuid()).pw_dir).resolve()
    profile=['(version 1)','(allow default)','(deny network*)','(deny file-write*)','(allow file-write* (literal "/dev/null") (subpath '+json.dumps(str(oracle_output))+'))','(deny file-read-data (subpath '+json.dumps(str(account_home))+'))','(allow file-read-data (subpath '+json.dumps(str(workspace))+'))']
    for binding in oracle['bound_files']:
        profile.append('(allow file-read-data (literal '+json.dumps(str(Path(binding['path']).resolve()))+'))')
    (oracle_output/'sandbox.sb').write_text('\n'.join(profile)+'\n')
    oracle_argv=['/usr/bin/sandbox-exec','-f',str(oracle_output/'sandbox.sb'),*argv]
    oracle_env={'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','PYTHONDONTWRITEBYTECODE':'1','PYTHONPYCACHEPREFIX':'/dev/null','TMPDIR':str(oracle_output)}
    namespace_command = None
    if a.container_runtime:
        from container_native import run_oracle as isolated_oracle
        completed, oracle_argv, namespace_command = isolated_oracle(a.container_runtime.resolve(), workspace, oracle_output, label.removeprefix('oracle-'), argv, oracle['bound_files'])
    else:
        completed=subprocess.run(oracle_argv,cwd=workspace,env=oracle_env,capture_output=True,timeout=min(oracle.get('timeout_seconds',60),120))
    (run/(label+'.stdout')).write_bytes(completed.stdout)
    (run/(label+'.stderr')).write_bytes(completed.stderr)
    return {'argv':oracle_argv,'exit_code':completed.returncode,'stdout_path':str(run/(label+'.stdout')),'stdout_sha256':sha(completed.stdout),'stderr_path':str(run/(label+'.stderr')),'stderr_sha256':sha(completed.stderr),'bound_files':oracle['bound_files'],'namespace_command':namespace_command}
feedback_manifest = None
if a.feedback_json:
    from feedback import manifest as freeze_feedback_manifest
    feedback_manifest = freeze_feedback_manifest(json.loads(a.feedback_json), workspace, files, editable)
    (run/'feedback-manifest.json').write_text(json.dumps(feedback_manifest,indent=2)+'\n')
oracle_before=run_oracle('oracle-before')
argv=[sys.executable,'-B',str(Path(__file__).with_name('invoke.py')),a.agent,'--workspace',str(workspace),'--output',str(run/'native'),'--task-file',str(run/'task.txt'),'--iterations',str(a.iterations),'--command',a.command,'--files',*editable,'--local-model-port',str(a.local_model_port)]
if pr_agent_options:
    argv += ['--pr-agent-json',json.dumps(pr_agent_options,separators=(',',':'))]
if mini_options:
    argv += ['--mini-json',json.dumps(mini_options,separators=(',',':'))]
if openhands_options:
    argv += ['--openhands-json',json.dumps(openhands_options,separators=(',',':'))]
if aider_mode:
    argv += ['--aider-json',json.dumps(aider_mode,separators=(',',':'))]
if feedback_manifest:
    argv += ['--feedback-manifest', str(run/'feedback-manifest.json')]
if a.model_provenance:
    argv += ['--model-provenance',str(a.model_provenance.resolve())]
if a.worker_owned_group:
    argv += ['--worker-owned-group']
if a.container_runtime:
    argv += ['--container-runtime',str(a.container_runtime.resolve())]
if a.interrupt_after_observations is not None:
    argv += ['--interrupt-after-observations',str(a.interrupt_after_observations)]
if a.conversation_id:
    argv += ['--conversation-id', a.conversation_id]
# T-4-1 (R04, Q11): this 630-second bound is the product's own deadline for the
# native agent, 30 seconds inside the worker's 660, so the capture can still be
# written. When it fires, the attempt TIMED OUT: say so in a capture the worker
# keeps, rather than letting TimeoutExpired escape and leave an empty report that
# reads downstream as a completed run whose output could not be parsed.
NATIVE_DEADLINE_SECONDS=630
with (run/'bridge.stdout.log').open('wb') as stdout,(run/'bridge.stderr.log').open('wb') as stderr:
    try:
        result=subprocess.run(argv,stdout=stdout,stderr=stderr,timeout=NATIVE_DEADLINE_SECONDS)
    except subprocess.TimeoutExpired:
        envelope={'schema':'thruline.local-agent-capture.v1','repository_id':a.agent,
            'process':{'exit_code':None,'state':'timed_out','elapsed_seconds':NATIVE_DEADLINE_SECONDS,
                       'deadline_seconds':NATIVE_DEADLINE_SECONDS,
                       'ended_by':'the native agent deadline in report.py; subprocess.run killed invoke.py'},
            'native_artifacts':[],'native_bytes_omitted':[],'complete':False,'acceptance_state':'not_evaluated',
            'limitations':['The native agent did not finish before its deadline and was stopped. Nothing it produced is a review.']}
        (run/'capture.json').write_text(json.dumps(envelope,indent=2)+'\n')
        print(json.dumps(envelope))
        sys.stdout.flush()
        raise SystemExit(124)
try:
    for name in files:
        selected=workspace/name
        if selected.exists() or selected.is_symlink():safe_artifact_bytes(selected,workspace)
    for path in (run/'native').rglob('*'):
        if path.is_symlink():raise ValueError('symlink rejected at native artifact boundary: '+str(path))
        if path.is_file():safe_artifact_bytes(path,run/'native')
except (ValueError,OSError) as error:
    (run/'capture-rejection.json').write_text(json.dumps({'schema':'thruline.local-agent-capture-rejection.v1','reason':str(error),'state':'rejected','native_process_exit_code':result.returncode,'native_output_path':str(run/'native')},indent=2)+'\n')
    raise SystemExit('agent capture rejected: '+str(error))
invocation=json.loads((run/'native/invocation.json').read_text())
patch=git('diff','--no-ext-diff','--no-textconv','--binary','HEAD')
(run/'proposal.patch').write_bytes(patch)
after=inventory()
oracle_after=run_oracle('oracle-after')
native=[]; total=0; omitted=[]
for path in sorted((run/'native').rglob('*')):
    if not path.is_file() or any(part in ['temp','cache','config','profiles'] for part in path.relative_to(run/'native').parts): continue
    data=safe_artifact_bytes(path,run/'native'); entry={'path':str(path),'relative_path':path.relative_to(run).as_posix(),'sha256':sha(data),'size':len(data)}
    if len(data)<=512*1024 and total+len(data)<=2*1024*1024:
        entry['bytes_base64']=base64.b64encode(data).decode(); total+=len(data)
    else: omitted.append(entry['relative_path'])
    native.append(entry)
original_unchanged=all(((source/name).read_bytes() if (source/name).is_file() else None)==data for name,data in original.items())
status=invocation['state']
native_status=None
trajectory=run/'native/trajectory.json'
state=run/'native/native-state.json'
if trajectory.is_file(): native_status=json.loads(trajectory.read_text()).get('info',{}).get('exit_status')
elif state.is_file(): native_status=json.loads(state.read_text()).get('execution_status')
if invocation['exit_code']!=0 and status=='exited': status='failed'
changed=[name for name in files if ((workspace/name).read_bytes() if (workspace/name).is_file() else None)!=captured[name]]
generated=[path.relative_to(workspace).as_posix() for path in workspace.rglob('*') if path.is_file() and '.git' not in path.relative_to(workspace).parts and path.relative_to(workspace).as_posix() not in files]
allchanged=git('status','--porcelain','--untracked-files=all').decode()
limits=['Selected files only; files outside the explicitly accepted scope were not supplied.',
    'Model-generated review and changes are advisory until independent oracle verification.',
    'No proposal was applied to the original source tree.']
if omitted: limits.append('Some native artifacts exceed the bounded inline envelope; full bytes remain at the recorded paths.')
envelope={'schema':'thruline.local-agent-capture.v1','repository_id':a.agent,
    'tool_source_commit':invocation['installation']['source_commit'],
    'source':{'root':str(source),'head':head,'base_commit':base,'selected_files':files,'before':before,'after':after,'original_unchanged':original_unchanged},
    'model':invocation.get('model_provenance'), 'task_sha256':sha(task),
    'process':{'exit_code':invocation['exit_code'],'state':status,'elapsed_seconds':invocation['elapsed_seconds']},
    'agent_termination':native_status,'acceptance_state':'not_evaluated',
    'proposal':{'path':str(run/'proposal.patch'),'sha256':sha(patch),'size':len(patch),'bytes_base64':base64.b64encode(patch).decode(),'changed_files':changed,'git_status':allchanged},
    'native_artifacts':native,'native_bytes_omitted':omitted,'limitations':limits,
    'complete':status=='exited' and original_unchanged and not omitted,
    'invocation_path':str(run/'native/invocation.json')}
envelope['source'].update(capture_basis='committed_HEAD' if a.agent=='R14' else 'working_files',editable_files=editable,read_only_context_files=sorted(set(files)-set(editable)),dirty_worktree_paths=list(filter(None,dirty_paths)),excluded_untracked_paths=list(filter(None,untracked_paths)),generated_outside_selected_scope=generated)
if aider_mode and aider_mode['mode'] == 'ask':
    envelope['source'].update(editable_files=[],read_only_context_files=files)
if pr_agent_options:
    envelope['source'].update(capture_basis='unified_diff_file',editable_files=[],read_only_context_files=files)
    envelope['pr_agent_options']=pr_agent_options
    scope_path=run/'native/native-diff-scope.json'
    envelope['pr_agent_plain_diff']=json.loads(safe_artifact_bytes(scope_path,run/'native')) if scope_path.is_file() else None
    if envelope['pr_agent_plain_diff'] is None:
        envelope['complete']=False
    envelope['oracle_applicability']={'state':'not_applicable_to_patch_only_review','reason':'The plain-diff provider receives no complete source tree and produces advisory review text. The registered source-repair oracle was not run and no behavior acceptance is claimed.'}
    limits.append('Plain-diff provider receives only the supplied patch. Unchanged files, repository enrichment and commit history are not available; review text is advisory.')
envelope['aider_mode']=aider_mode
envelope['openhands_options']=openhands_options
if mini_options:
    harness_path=run/'native/mini-harness.json'
    harness_bytes=safe_artifact_bytes(harness_path,run/'native') if harness_path.is_file() else None
    envelope['mini_harness']={'options':mini_options,'native':json.loads(harness_bytes) if harness_bytes else None,'source_artifact':str(harness_path) if harness_bytes else None,'source_sha256':sha(harness_bytes) if harness_bytes else None,'capture_error':None if harness_bytes else 'Native harness summary unavailable'}
envelope['native_feedback']=feedback_manifest
envelope['oracle']={'before':oracle_before,'after':oracle_after,'config_sha256':sha(a.oracle_config.read_bytes()) if a.oracle_config else None}
try:
    envelope['proposal']['diff']=patch.decode('utf-8')
except UnicodeDecodeError:
    envelope['proposal']['diff']=None
envelope['proposal']['binary']=envelope['proposal']['diff'] is None or b'GIT binary patch' in patch
for relative,kind in [('review.md','native_review'),('improve.md','native_suggestions'),('description.md','native_description'),('chat-history.md','native_chat_transcript'),('stdout.log','native_stdout')]:
    native_path=run/'native'/relative
    if native_path.is_file() and native_path.stat().st_size:
        content=native_path.read_bytes(); text=content.decode('utf-8',errors='replace')
        start=max(0,len(text)-12000)
        envelope['native_output']={'kind':kind,'text':text[start:],'source_artifact':str(native_path),'source_sha256':sha(content),'truncated':start>0,'char_start':start,'char_end':len(text)}
        break
if trajectory.is_file():
    trajectory_data=json.loads(trajectory.read_text())
    assistant=[m for m in trajectory_data.get('messages',[]) if m.get('role')=='assistant']
    if assistant:
        text=str(assistant[-1].get('content',''))
        envelope['native_output']={'kind':'native_last_assistant_message','text':text[:12000],'source_artifact':str(trajectory),'source_sha256':sha(trajectory.read_bytes()),'truncated':len(text)>12000,'json_pointer':'/messages/'+str(trajectory_data['messages'].index(assistant[-1]))+'/content'}
events_path=run/'native/events.jsonl'
if events_path.is_file():
    events_bytes=safe_artifact_bytes(events_path,run/'native')
    for line_number,line in reversed(list(enumerate(events_bytes.decode('utf-8').splitlines(),start=1))):
        event=json.loads(line)
        if event.get('source')!='agent':continue
        text=None; pointer=None
        if event.get('kind')=='MessageEvent':
            for index,content in enumerate(event.get('llm_message',{}).get('content',[])):
                if content.get('type')=='text' and content.get('text'):
                    text=content['text']; pointer='/llm_message/content/'+str(index)+'/text'; break
        elif event.get('action',{}).get('kind')=='FinishAction':
            text=event['action'].get('message'); pointer='/action/message'
        if text:
            envelope['native_output']={'kind':'native_last_assistant_message','text':text[:12000],'source_artifact':str(events_path),'source_sha256':sha(events_bytes),'truncated':len(text)>12000,'jsonl_line':line_number,'json_pointer':pointer}
            break
if oracle_after:
    envelope['acceptance_state']='oracle_passed' if oracle_after['exit_code']==0 else 'oracle_failed'
(run/'capture.json').write_text(json.dumps(envelope,indent=2)+'\n')
print(json.dumps(envelope))
raise SystemExit(0 if status=='exited' else 2)
