"""Pinned native Aider modes; model prose never grants product acceptance."""
import hashlib
import json
from pathlib import Path
import time

SCHEMA = 'thruline.aider-mode.v1'
EDIT_FORMATS = ('diff', 'diff-fenced', 'whole', 'patch', 'udiff', 'udiff-simple',
                'editor-diff', 'editor-whole', 'editor-diff-fenced')
DEFAULT = {'schema': SCHEMA, 'mode': 'edit', 'edit_format': 'diff', 'max_reflections': 3}


def validate(value):
    if value is None:
        return dict(DEFAULT)
    if not isinstance(value, dict) or set(value) != {'schema', 'mode', 'edit_format', 'max_reflections'}:
        raise ValueError('Aider mode requires only schema, mode, edit_format and max_reflections')
    if value['schema'] != SCHEMA or value['mode'] not in ('ask', 'architect', 'edit'):
        raise ValueError('Unsupported native Aider mode contract')
    if type(value['max_reflections']) is not int or not 0 <= value['max_reflections'] <= 6:
        raise ValueError('Native Aider reflections must be an integer from 0 to 6')
    if value['mode'] == 'ask':
        if value['edit_format'] is not None:
            raise ValueError('Ask mode has no editable response format')
    elif value['edit_format'] not in EDIT_FORMATS:
        raise ValueError('Unsupported native Aider edit format')
    return dict(value)


def native_options(spec, model):
    spec = validate(spec)
    # Every main, summarizer and architect editor request uses the same
    # separately verified local model and the fixed local API endpoint.
    if not isinstance(model, str) or not model or any(char in model for char in '\r\n\x00/'):
        raise ValueError('Native model name must be the verified local model tag')
    name = 'openai/' + model
    args = ['--edit-format', spec['mode'] if spec['mode'] != 'edit' else spec['edit_format'],
            '--weak-model', name, '--editor-model', name, '--no-suggest-shell-commands']
    if spec['mode'] == 'architect':
        # Native automatic editor dispatch changes only the disposable selected
        # workspace. Applying any resulting proposal still requires product review.
        args += ['--auto-accept-architect', '--editor-edit-format', spec['edit_format']]
    return args


def run_mode(coder, files, output, spec, execute):
    """Observe genuine native calls, including architect editor and repair clones."""
    from aider.coders.base_coder import Coder
    from feedback import read_regular
    spec = validate(spec)
    output = Path(output)
    original_run, original_bound = Coder.run, Coder.max_reflections
    calls, limited = [], []

    def event(kind, value):
        with (output/'aider-mode-events.jsonl').open('a') as stream:
            stream.write(json.dumps({'kind': kind, 'monotonic_seconds': time.monotonic(), **value}, default=str) + '\n')

    def source():
        hashes, errors = {}, {}
        for name in files:
            try:
                hashes[name] = hashlib.sha256(read_regular(Path(coder.root)/name, 8*1024*1024)).hexdigest()
            except (OSError, ValueError) as error:
                errors[name] = {'type': type(error).__name__, 'message': str(error)}
        return {'sha256': hashes, 'errors': errors}

    def native_run(self, *args, **kwargs):
        identity = len(calls)
        row = {'call': identity, 'native_class': type(self).__name__, 'edit_format': self.edit_format,
               'model': self.main_model.name, 'max_reflections': self.max_reflections,
               'before': source()}
        calls.append(row)
        event('Coder.run.enter', {**row, 'arguments': args, 'keyword_arguments': kwargs})
        try:
            result = original_run(self, *args, **kwargs)
        except BaseException as error:
            event('Coder.run.exception', {'call': identity, 'type': type(error).__name__, 'message': str(error), 'after': source()})
            raise
        exhausted = bool(self.reflected_message and self.num_reflections >= self.max_reflections)
        finished = {'call': identity, 'native_class': type(self).__name__, 'edit_format': self.edit_format,
                    'result': result, 'num_reflections': self.num_reflections,
                    'max_reflections': self.max_reflections, 'reflected_message': self.reflected_message,
                    'reflection_limit_reached': exhausted, 'after': source()}
        if exhausted:
            limited.append(identity)
        event('Coder.run.return', finished)
        row.update({key: finished[key] for key in ['num_reflections', 'reflection_limit_reached', 'after']})
        return result

    # The upstream bound is a class attribute. Applying it here includes native
    # architect editors and lint-repair clones, not only the initial coder.
    Coder.max_reflections, Coder.run = spec['max_reflections'], native_run
    try:
        result = execute()
        summary = {'schema': 'thruline.aider-native-mode-result.v1', 'mode': spec,
                   'native_calls': calls, 'reflection_limit_reached': bool(limited),
                   'limited_calls': limited, 'acceptance_state': 'not_evaluated'}
        (output/'aider-mode-result.json').write_text(json.dumps(summary, indent=2) + '\n')
        return result, summary
    finally:
        Coder.run, Coder.max_reflections = original_run, original_bound
