"""Validate and freeze native development feedback; never an acceptance oracle."""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shlex
import stat

LIMIT = 1024 * 1024


def relative_name(value):
    if not isinstance(value, str) or not value or len(value) > 4096:
        raise ValueError('feedback test path must be a bounded relative string')
    path = PurePosixPath(value)
    if path.is_absolute() or value != path.as_posix() or any(
            part in ('', '.', '..', '.git') for part in value.split('/')) or any(
            character in value for character in '\\\x00\r\n'):
        raise ValueError('feedback test path must remain within selected source')
    return value


def validate(spec, selected, editable):
    if not isinstance(spec, dict) or set(spec) != {'schema', 'mode', 'test_file', 'command'}:
        raise ValueError('feedback requires only schema, mode, test_file and command')
    if spec['schema'] != 'thruline.aider-feedback.v1' or spec['mode'] not in ('lint-and-test', 'test'):
        raise ValueError('unsupported native feedback contract or mode')
    name = relative_name(spec['test_file'])
    if name not in selected or name in editable:
        raise ValueError('feedback test must be selected read-only context, outside edit_files')
    command = spec['command']
    if not isinstance(command, list) or not 2 <= len(command) <= 16 or any(
            not isinstance(value, str) or not value or len(value) > 4096 or any(
                char in value for char in '\x00\r\n') for value in command):
        raise ValueError('feedback command must be a bounded explicit argv list')
    if command[0] not in ('python', 'python3') or command.count('{feedback}') != 1:
        raise ValueError('feedback requires the pinned Python runtime and one {feedback} argument')
    index = command.index('{feedback}')
    if any(flag not in ('-B', '-I', '-u') for flag in command[1:index]):
        raise ValueError('feedback interpreter options may not replace the selected script')
    for arg in command[index + 1:]:
        if ('{' in arg or '}' in arg) and arg != '{workspace}':
            raise ValueError('unknown feedback command substitution')
    return json.loads(json.dumps(spec))


def read_regular(path, limit=LIMIT):
    path = Path(path).absolute()
    if any(part.is_symlink() for part in [path, *path.parents]):
        raise ValueError('feedback file or parent is a symlink')
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > limit:
            raise ValueError('feedback must be a bounded regular file')
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError('feedback grew beyond its byte bound')
    return data


def binding(path):
    data = read_regular(path)
    return {'path': str(path), 'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data)}


def manifest(spec, workspace, selected, editable):
    spec = validate(spec, selected, editable)
    workspace = Path(workspace).resolve(strict=True)
    source = workspace / spec['test_file']
    return {'schema': 'thruline.frozen-aider-feedback.v1', 'config': spec,
            'selected_files': list(selected), 'editable_files': list(editable),
            'source': binding(source)}


def freeze(value, workspace, destination, editable):
    if not isinstance(value, dict) or value.get('schema') != 'thruline.frozen-aider-feedback.v1':
        raise ValueError('unsupported frozen feedback manifest')
    if value.get('editable_files') != list(editable):
        raise ValueError('feedback edit scope differs from the native invocation')
    spec = validate(value['config'], value['selected_files'], editable)
    workspace = Path(workspace).resolve(strict=True)
    path = workspace / spec['test_file']
    source = value['source']
    if source.get('path') != str(path):
        raise ValueError('feedback binding points outside its selected captured source')
    data = read_regular(path)
    if hashlib.sha256(data).hexdigest() != source.get('sha256') or len(data) != source.get('size'):
        raise ValueError('feedback bytes changed since the source capture')
    destination = Path(destination)
    if any(part.is_symlink() for part in [destination, *destination.parents]):
        raise ValueError('feedback destination may not contain symlinks')
    destination.mkdir(exist_ok=False)
    (destination / 'check.py').write_bytes(data)
    prepared = {'schema': 'thruline.prepared-aider-feedback.v1', 'config': spec,
                'source': source, 'sha256': source['sha256'], 'size': source['size'],
                'scope': 'Frozen development feedback only; independent acceptance remains separate.'}
    (destination / 'manifest.json').write_text(json.dumps(prepared, indent=2) + '\n')
    return prepared


def guest(directory):
    directory = Path(directory)
    value = json.loads(read_regular(directory / 'manifest.json', 32768))
    if value.get('schema') != 'thruline.prepared-aider-feedback.v1':
        raise ValueError('unsupported prepared feedback manifest')
    spec = validate(value['config'], [value['config']['test_file']], [])
    data = read_regular(directory / 'check.py')
    if hashlib.sha256(data).hexdigest() != value['sha256'] or len(data) != value['size']:
        raise ValueError('read-only feedback bytes differ from their binding')
    replacements = {'{feedback}': str(directory / 'check.py'), '{workspace}': '/workspace'}
    argv = [replacements.get(arg, arg) for arg in spec['command']]
    return spec['mode'], argv, shlex.join(argv)


@contextmanager
def record_aider_callbacks(coder, files, output):
    """Retain actual native callback results without replacing their behavior."""
    from aider.linter import Linter
    from aider.commands import Commands
    import aider.commands as native_commands
    import aider.linter as native_linter
    output = Path(output)

    def event(kind, value):
        sources, source_errors = {}, {}
        for name in files:
            try:
                sources[name] = hashlib.sha256(read_regular(Path(coder.root) / name, 8 * LIMIT)).hexdigest()
            except (OSError, ValueError) as error:
                source_errors[name] = {'type': type(error).__name__, 'message': str(error)}
        record = {'kind': kind, 'source_sha256': sources, 'source_errors': source_errors, **value}
        with (output / 'feedback-native-events.jsonl').open('a') as stream:
            stream.write(json.dumps(record, default=str) + '\n')

    original_lint, original_test, original_command = Linter.lint, Commands.cmd_test, native_commands.run_cmd
    original_lint_subprocess = native_linter.subprocess

    def recorded_lint(self, *args, **kwargs):
        result = original_lint(self, *args, **kwargs)
        event('Linter.lint', {'arguments': args, 'result': result})
        return result

    def recorded_test(self, *args, **kwargs):
        result = original_test(self, *args, **kwargs)
        event('Commands.cmd_test', {'arguments': args, 'result': result})
        return result

    def recorded_command(*args, **kwargs):
        result = original_command(*args, **kwargs)
        event('aider.run_cmd.run_cmd', {'arguments': args, 'cwd': kwargs.get('cwd'),
                                       'exit_code': result[0], 'combined_output': result[1]})
        return result

    class RecordedLintSubprocess:
        def __getattr__(self, name):
            return getattr(original_lint_subprocess, name)

        def run(self, *args, **kwargs):
            result = original_lint_subprocess.run(*args, **kwargs)
            event('aider.linter.subprocess.run', {'arguments': args, 'cwd': kwargs.get('cwd'),
                                                'exit_code': result.returncode,
                                                'stdout': result.stdout, 'stderr': result.stderr})
            return result

    Linter.lint, Commands.cmd_test, native_commands.run_cmd = recorded_lint, recorded_test, recorded_command
    native_linter.subprocess = RecordedLintSubprocess()
    try:
        yield event
    finally:
        Linter.lint, Commands.cmd_test, native_commands.run_cmd = original_lint, original_test, original_command
        native_linter.subprocess = original_lint_subprocess


def run_aider(coder, files, output, mode, task):
    """Use native lint/test feedback and deliver the accepted task to the model."""
    with record_aider_callbacks(coder, files, output) as event:
        event('feedback-start', {'mode': mode, 'auto_lint': coder.auto_lint,
                                 'auto_test': coder.auto_test, 'test_cmd': coder.test_cmd})
        if mode == 'lint-and-test':
            coder.commands.cmd_lint(fnames=files)
        coder.commands.cmd_test(coder.test_cmd)
        # cmd_test itself adds genuine failed output to native conversation
        # context. Always deliver the accepted task as a real user input too.
        coder.run(with_message=task)
        lint = ({name: coder.linter.lint(coder.abs_root_path(name)) for name in files}
                if mode == 'lint-and-test' else {})
        test = coder.commands.cmd_test(coder.test_cmd)
        return {'mode': mode, 'final_lint': lint, 'final_test_errors': test,
                'unresolved_native_feedback': bool(test or any(lint.values()))}
