"""Pinned local-agent Linux execution; the product owns namespace cleanup.

Only the trusted host broker holds the local Docker socket. Native tools receive
no host network/IPC, credentials, or arbitrary host mounts. Model bodies are
forwarded unchanged through a bounded mailbox to the single registered endpoint.
"""
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import threading
import time
import urllib.request

SOURCE_PINS = {
    'R12': 'a4be6ccd87ebaa59b361f3f028d116ce1761b626',
    'R13': '50080b58d35b4824fda25fca2345d80bcd08aeff',
    'R14': 'f3b385ea2927247ddcff2fe252472380b9c8f5fc',
    'R15': 'a83fcae82d2a08f0ee0c688f9d137b3566c097f8',
}


# The stops the product itself enforces on a native run: its own deadline and
# its own output bound. When one of them fired, that IS why the run ended.
PRODUCT_STOPS = ('timed_out', 'output_limit')


def run_state(product_stop, broker_alive, transport_errors):
    """The one state a native container run ends in (T-4-1 fix 1, R04, Q11).

    A stop the product enforced wins over a transport failure: killing the
    container at the deadline is what leaves a model request unanswered or the
    broker still draining, so calling that run `transport_failed` would hide
    the product's own deadline behind a failure the deadline caused. Pure, so
    the precedence is testable without Docker.
    """
    if product_stop is not None:
        if product_stop not in PRODUCT_STOPS: raise ValueError('unknown product stop: ' + str(product_stop))
        return product_stop
    if broker_alive or transport_errors: return 'transport_failed'
    return 'exited'


def sha(data): return hashlib.sha256(data).hexdigest()
def write(path, value): path.write_text(json.dumps(value, indent=2) + '\n')
def ref(path):
    data = secure(path, 16 * 1024 * 1024)
    return {'path': str(path), 'sha256': sha(data), 'size': len(data)}

def secure(path, limit):
    path = Path(path)
    if any(p.is_symlink() for p in [path, *path.parents]): raise ValueError('symlink in container input/artifact')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit: raise ValueError('unbounded/nonregular container input')
        data = stream.read(limit + 1)
    if len(data) > limit: raise ValueError('container artifact exceeds bound')
    return data


def runtime(path):
    value = json.loads(secure(path, 1024 * 1024))
    if value['schema'] != 'thruline.local-agent-container-runtime.v1' or value.get('source_commit') != SOURCE_PINS.get(value.get('repository_id')):
        raise ValueError('unprovisioned native container provider/source revision')
    docker_path = Path(value['docker']['path'])
    if any(path.is_symlink() for path in [docker_path, *docker_path.parents]): raise ValueError('Docker runtime symlink changed')
    with docker_path.open('rb') as stream:
        if os.fstat(stream.fileno()).st_size != value['docker']['size']: raise ValueError('Docker runtime size changed')
        actual_docker = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual_docker != value['docker']['sha256']: raise ValueError('Docker runtime digest changed')
    for binding in value['provenance']:
        path = Path(binding['path'])
        if any(part.is_symlink() for part in [path, *path.parents]):
            raise ValueError('native provenance contains a symlink')
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, 'rb') as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_size != binding['size'] or info.st_size > 256 * 1024 * 1024:
                raise ValueError('native provenance size or file type changed')
            current = hashlib.file_digest(stream, 'sha256').hexdigest()
        if current != binding['sha256']: raise ValueError('native container runtime provenance changed')
    argv = [value['docker']['path'], '--host', 'unix://' + value['socket']]
    result = subprocess.run([*argv, 'image', 'inspect', value['image']], capture_output=True, timeout=10, env={'PATH': '/usr/bin:/bin'})
    if result.returncode:
        # Docker diagnostics contain only this fixed image lookup, not workload
        # command output or provider secrets. Preserve the actual failure.
        raise ValueError('pinned native container image is unavailable; image inspect exit=' +
                         str(result.returncode) + '; stderr=' + result.stderr[:8192].decode('utf-8', 'replace'))
    image = json.loads(result.stdout)[0]
    if image['Id'] != value['image']: raise ValueError('native image identity mismatch')
    return value, argv, image


def owned_command(manifest, suffix, output):
    value, docker, image = runtime(manifest)
    owner = os.environ.get('THRULINE_BROKER_CONTAINER_OWNER', '')
    name = os.environ.get('THRULINE_BROKER_CONTAINER_NAME', '')
    if not re.fullmatch(r'thruline-agent-owner-[a-zA-Z0-9_-]+', owner) or owner != name:
        raise ValueError('actual product namespace ownership is required')
    memory = int(os.environ['THRULINE_BROKER_MEMORY_MB']); cpu = int(os.environ['THRULINE_BROKER_CPU_SECONDS'])
    if not 128 <= memory <= 16384 or not 1 <= cpu <= 3600: raise ValueError('invalid native resource limits')
    name += '-' + suffix
    args = [*docker, 'run', '--pull=never', '--name', name, '--label', 'thruline.owner=' + owner,
            '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges',
            '--pids-limit=128', '--memory=' + str(memory) + 'm', '--memory-swap=' + str(memory) + 'm', '--cpus=2',
            '--ulimit=cpu=' + str(cpu) + ':' + str(cpu), '--user=' + str(os.getuid()) + ':' + str(os.getgid()),
            '--tmpfs=/tmp:rw,nosuid,nodev,size=134217728,mode=1777', '--env=PYTHONDONTWRITEBYTECODE=1',
            '--env=PYTHONNOUSERSITE=1', '--env=HOME=/tmp/home', '--entrypoint=python']
    write(output / 'runtime-identity.json', {'manifest': ref(manifest), 'image': image, 'repository_commit': value['source_commit']})
    return value, docker, name, args


def mount(args, host, guest, readonly=True):
    host = Path(host)
    if any(char in str(host) for char in ',\n\r') or any(p.is_symlink() for p in [host, *host.parents]): raise ValueError('unsafe container mount path')
    args.extend(['--mount', 'type=bind,src=' + str(host) + ',dst=' + guest + (',readonly' if readonly else '')])


def inspect(docker, name, output):
    result = subprocess.run([*docker, 'inspect', name], capture_output=True, timeout=10, env={'PATH': '/usr/bin:/bin'})
    (output / 'container-inspect.json').write_bytes(result.stdout)
    if result.returncode: raise ValueError('native container final state unavailable')
    value = json.loads(result.stdout)[0]
    if value['State']['Running'] or value['State']['Pid'] != 0: raise ValueError('native namespace is still running')
    return value


def run_oracle(manifest, workspace, output, label, argv, bound_files):
    output.mkdir(exist_ok=True)
    value, docker, name, args = owned_command(manifest, 'oracle-' + label, output)
    mount(args, workspace, '/workspace')
    mapped = {str(workspace): '/workspace'}
    for index, binding in enumerate(bound_files):
        path = Path(binding['path']); data = secure(path, 16 * 1024 * 1024)
        if sha(data) != binding['sha256']: raise ValueError('protected oracle changed')
        guest = '/oracle/' + str(index) + '-' + path.name; mount(args, path, guest); mapped[str(path)] = guest
    translated = [mapped.get(str(arg), str(arg)) for arg in argv[1:]]
    if any(arg.startswith('/Users/') for arg in translated): raise ValueError('oracle command refers to an unmounted host input')
    # Isolated (-I) oracle commands ignore Python environment variables.
    # Force source loading before all caller-supplied interpreter flags.
    args += [value['image'], '-B', '-X', 'pycache_prefix=/dev/null', *translated]
    started = time.monotonic()
    try:
        result = subprocess.run(args, capture_output=True, timeout=120, env={'PATH': '/usr/bin:/bin'})
    except subprocess.TimeoutExpired:
        subprocess.run([*docker, 'kill', name], capture_output=True, timeout=10, env={'PATH': '/usr/bin:/bin'})
        raise
    final = inspect(docker, name, output)
    write(output / 'command.json', {'argv': args, 'exit_code': result.returncode, 'elapsed_seconds': time.monotonic() - started, 'state': final['State'], 'oracle_bindings': bound_files})
    return result, args, ref(output / 'command.json')


def run_native(manifest, workspace, output, model, model_port, task, files, iterations, repository='R15', command='review', resume_id=None, interrupt_after_observations=None, conversation_id=None, feedback_directory=None, aider_mode=None, openhands_options=None, pr_agent_options=None, mini_options=None):
    if model_port not in (11435, 11436):
        raise ValueError('unregistered model endpoint')
    if mini_options is not None:
        from mini_harness import validate as validate_mini
        mini_options = validate_mini(mini_options)
        if repository != 'R15': raise ValueError('mini-SWE harness belongs to R15 only')
    if openhands_options is not None:
        from openhands_options import validate
        openhands_options = validate(openhands_options)
        if repository != 'R13': raise ValueError('OpenHands options belong to R13 only')
    if pr_agent_options is not None:
        from pr_agent_options import validate as validate_pr_agent
        pr_agent_options = validate_pr_agent(pr_agent_options, files)
        if repository != 'R14': raise ValueError('Plain-diff options belong to PR-Agent only')
    value, docker, name, args = owned_command(manifest, 'native', output)
    if value['repository_id'] != repository:
        raise ValueError('native image belongs to another provider')
    if aider_mode is not None:
        if repository != 'R12':raise ValueError('native modes belong to Aider only')
        from aider_modes import validate as validate_aider_mode
        aider_mode = validate_aider_mode(aider_mode)
        if aider_mode['mode'] == 'ask' and feedback_directory is not None:raise ValueError('Ask mode cannot run repair feedback')
    native = output / 'container-native'; native.mkdir(); mailbox = output / 'mailbox'; mailbox.mkdir(); wire = output / 'model-wire'; wire.mkdir()
    task_data = secure(task, 1024 * 1024); (native / 'task.txt').write_bytes(task_data)
    (native / 'empty.env').touch(); (native / 'empty.yml').write_text('{}\n')
    if resume_id:
        if repository != 'R13' or not (output / 'persistence').is_dir():
            raise ValueError('actual OpenHands persistence is required for native resume')
        shutil.copytree(output / 'persistence', native / 'persistence')
    drivers = output / 'driver-source'
    # This is the disposable captured workspace, never the user's repository.
    # Native editors need sibling temporary files and atomic rename. Preserve
    # Git and captured context with read-only overlays; validate every remaining
    # generated/changed path when the host captures the proposal.
    mount(args, workspace, '/workspace', readonly=bool((aider_mode and aider_mode['mode'] == 'ask') or pr_agent_options))
    mount(args, workspace / '.git', '/workspace/.git')
    for path in workspace.rglob('*'):
        relative = path.relative_to(workspace)
        if '.git' in relative.parts:
            continue
        if path.is_symlink():
            raise ValueError('symlink in captured workspace')
        if path.is_file() and relative.as_posix() not in files:
            mount(args, path, '/workspace/' + relative.as_posix())
    for file in files:
        path = workspace / file
        if not path.is_file() or not path.is_relative_to(workspace): raise ValueError('writable source is outside captured file selection')
    mount(args, native, '/native', readonly=False); mount(args, native/'task.txt', '/native/task.txt'); mount(args, mailbox, '/mailbox', readonly=False); mount(args, drivers, '/drivers')
    if feedback_directory is not None:
        if repository != 'R12':
            raise ValueError('native feedback belongs to Aider only')
        feedback_directory = Path(feedback_directory)
        if feedback_directory != output/'feedback-input' or feedback_directory.is_symlink():
            raise ValueError('feedback mount must be the frozen invocation input directory')
        from feedback import guest as check_feedback
        check_feedback(feedback_directory)
        mount(args, feedback_directory, '/feedback')
    args += ['--env=THRULINE_NATIVE_NAMESPACE=linux', '--env=LITELLM_LOCAL_MODEL_COST_MAP=true', '--env=LITELLM_TELEMETRY=false', '--env=OTEL_SDK_DISABLED=true',
             '--env=HF_HUB_OFFLINE=1', '--env=TRANSFORMERS_OFFLINE=1', '--env=DO_NOT_TRACK=1', '--env=MSWEA_SILENT_STARTUP=1',
             value['image'], '/drivers/container_gateway.py', '--local-model-port', str(model_port), 'python', '/drivers/engine.py', repository, '--workspace', '/workspace',
             '--output', '/native', '--task-file', '/native/task.txt', '--model-name', model, '--local-model-port', str(model_port), '--iterations', str(iterations), '--command', command, '--files', *files]
    if feedback_directory is not None:
        args += ['--feedback-dir', '/feedback']
    if aider_mode is not None:
        args += ['--aider-json',json.dumps(aider_mode,separators=(',',':'))]
    if openhands_options is not None:
        args += ['--openhands-json', json.dumps(openhands_options,separators=(',',':'))]
    if pr_agent_options is not None:
        args += ['--pr-agent-json', json.dumps(pr_agent_options,separators=(',',':'))]
    if mini_options is not None:
        args += ['--mini-json', json.dumps(mini_options,separators=(',',':'))]
    if resume_id:
        args += ['--resume-id', resume_id]
    if conversation_id:
        args += ['--conversation-id', conversation_id]
    if interrupt_after_observations is not None:
        args += ['--interrupt-after-observations', str(interrupt_after_observations)]
    write(output / 'container-command.json', {'argv': args, 'runtime': ref(manifest), 'editable_files': [] if pr_agent_options else files, 'plain_diff_patch_only': bool(pr_agent_options), 'context_mount': 'private workspace writable for native temporary files; captured context and Git metadata read-only; generated paths require host scope validation'})
    stopped = threading.Event(); processed = set(); logs = []; errors = []; opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def broker():
        while not stopped.is_set():
            for path in sorted(mailbox.glob('*.request.json')):
                if path.name in processed: continue
                processed.add(path.name); identity = path.name.removesuffix('.request.json'); record = {'id': identity, 'received_at': time.time()}
                try:
                    if not re.fullmatch('[a-f0-9]{32}', identity): raise ValueError('invalid mailbox request identifier')
                    raw = secure(path, 3 * 1024 * 1024); request = json.loads(raw)
                    (wire / (identity + '.request.json')).write_bytes(raw)
                    if len(processed) > 32 or request['id'] != identity: raise ValueError('model request budget or identity violation')
                    method, endpoint = request['method'], request['path']
                    if (method, endpoint) not in [('POST', '/api/chat'), ('POST', '/api/show'), ('GET', '/api/tags'), ('POST', '/v1/chat/completions'), ('GET', '/v1/models')]: raise ValueError('request outside registered local model API')
                    body = base64.b64decode(request['body_base64'], validate=True)
                    if len(body) > 2 * 1024 * 1024 or sha(body) != request['body_sha256']: raise ValueError('request body mismatch')
                    if method == 'POST':
                        payload = json.loads(body); selected_model = payload.get('model', payload.get('name') if endpoint == '/api/show' else None)
                        if selected_model != model: raise ValueError('unregistered model requested')
                    (wire / (identity + '.request.body')).write_bytes(body)
                    req = urllib.request.Request('http://127.0.0.1:' + str(model_port) + endpoint, data=body if method == 'POST' else None, method=method, headers={'Content-Type': 'application/json'})
                    with opener.open(req, timeout=165) as response:
                        data = response.read(8 * 1024 * 1024 + 1); status = response.status; headers = list(response.headers.items())
                    if len(data) > 8 * 1024 * 1024: raise ValueError('model response exceeds bound')
                    (wire / (identity + '.response.body')).write_bytes(data)
                    result = {'id': identity, 'status': status, 'headers': headers, 'body_base64': base64.b64encode(data).decode(), 'body_sha256': sha(data)}
                    write(wire / (identity + '.response.json'), result)
                    record.update(endpoint=endpoint, model=model, request=ref(wire / (identity + '.request.json')), response=ref(wire / (identity + '.response.json')), response_delivered=not stopped.is_set())
                except Exception as error:
                    errors.append(str(error)); record['error'] = str(error)
                    data = json.dumps({'error': str(error), 'source': 'fixed-host-mailbox-policy'}).encode()
                    result = {'id': identity, 'status': 502, 'headers': [['Content-Type', 'application/json']], 'body_base64': base64.b64encode(data).decode(), 'body_sha256': sha(data), 'transport_error': True}
                if not stopped.is_set():
                    temporary = mailbox / (identity + '.response.new'); write(temporary, result); temporary.replace(mailbox / (identity + '.response.json'))
                else: record['late_response_discarded'] = True
                logs.append(record); write(output / 'model-wire-log.json', logs)
            stopped.wait(.025)
    thread = threading.Thread(target=broker, daemon=True); thread.start(); started = time.monotonic(); stop = None
    with (output / 'stdout.log').open('wb') as stdout, (output / 'stderr.log').open('wb') as stderr:
        process = subprocess.Popen(args, stdout=stdout, stderr=stderr, env={'PATH': '/usr/bin:/bin'})
        try:
            while process.poll() is None:
                if time.monotonic() - started > 590: stop = 'timed_out'; break
                if sum(path.stat().st_size for path in output.rglob('*') if path.is_file()) > 16 * 1024 * 1024: stop = 'output_limit'; break
                time.sleep(.2)
        finally:
            stopped.set()
            if process.poll() is None:
                subprocess.run([*docker, 'kill', name], capture_output=True, timeout=10, env={'PATH': '/usr/bin:/bin'})
            code = process.wait(timeout=15)
    thread.join(timeout=170)
    final = inspect(docker, name, output)
    state = run_state(stop, thread.is_alive(), errors)
    # Native bytes stay unchanged. Copy top-level native reports to the canonical
    # capture locations; retained raw originals and host wire remain available.
    reports = {
        'R12': ['aider-mode-config.json', 'aider-mode-events.jsonl', 'aider-mode-result.json', 'feedback-native-events.jsonl', 'feedback-native-config.json', 'aider-argv.json', 'repository-map.txt', 'repository-map-metadata.json', 'native-result.json', 'chat-history.md', 'llm-history.md', 'input-history.txt'],
        'R13': ['openhands-options.json', 'native-context-view.json', 'native-metrics.json', 'server-transport.jsonl', 'server-lifecycle.json', 'native-failure.json', 'conversation-id.txt', 'provider-inputs.jsonl', 'provider-responses.jsonl', 'events.jsonl', 'effective-task.txt', 'native-state.json', 'native-result.json', 'interruption-request.json', 'persistence', 'completions'],
        'R14': ['native-settings.json', 'native-result.json', 'model-inputs.jsonl', 'model-responses.jsonl', 'review.md', 'improve.md', 'description.md', 'input.diff', 'native-diff-scope.json', 'structured-review.json'],
        'R15': ['native-prompt-config.yaml', 'trajectory.json', 'native-result.json', 'mini-harness.json', 'native-harness-config.json', 'owned-instance.json', 'preds.json', 'native-exit-statuses.yaml'],
    }
    for name in reports[repository]:
        path = native / name
        if path.is_symlink():
            raise ValueError('native report is a symlink')
        if path.is_file():
            secure(path, 16 * 1024 * 1024)
            shutil.copyfile(path, output / path.name)
        elif path.is_dir():
            for item in path.rglob('*'):
                if item.is_symlink():
                    raise ValueError('native persisted output contains a symlink')
                if item.is_file():
                    secure(item, 16 * 1024 * 1024)
            if (output / name).exists():
                shutil.rmtree(output / name)
            shutil.copytree(path, output / name)
    write(output / 'container-result.json', {'state': state, 'exit_code': code, 'container': final, 'model_requests': len(processed), 'transport_errors': errors, 'broker_thread_terminated': not thread.is_alive()})
    return code if state == 'exited' else 5, state, {'backend': 'linux-native-namespace', 'observed_owned_processes_terminated': not final['State']['Running'] and final['State']['Pid'] == 0, 'final_state': final['State'], 'model_requests': len(processed), 'image': final['Image']}
