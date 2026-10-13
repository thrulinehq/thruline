"""Explicit patch-only context for the pinned native plain-diff provider."""
import hashlib
import json
import os
from pathlib import Path
import stat


def validate(value, files=None):
    if not isinstance(value, dict) or set(value) != {'schema', 'mode', 'diff_file'}:
        raise ValueError('PR-Agent plain-diff configuration requires schema, mode and diff_file')
    if value['schema'] != 'thruline.pr-agent-plain-diff.v1' or value['mode'] != 'plain-diff':
        raise ValueError('Unsupported explicit PR-Agent provider mode')
    name = value['diff_file']
    if not isinstance(name, str) or not name or '\x00' in name:
        raise ValueError('A relative UTF-8 unified diff file is required')
    path = Path(name)
    if path.is_absolute() or any(part in ['', '.', '..', '.git'] for part in name.split('/')):
        raise ValueError('Unsafe unified diff path')
    if files is not None and files != [name]:
        raise ValueError('Plain-diff scope must contain only the named diff file')
    return dict(value)


def read_input(workspace, config):
    config = validate(config)
    root = Path(workspace).resolve(strict=True)
    path = root
    for part in Path(config['diff_file']).parts:
        path = path / part
        if path.is_symlink():
            raise ValueError('Unified diff paths may not contain symlinks')
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, 'rb') as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > 1024 * 1024:
            raise ValueError('Unified diff must be a regular file no larger than 1 MiB')
        raw = stream.read(1024 * 1024 + 1)
    if len(raw) > 1024 * 1024:
        raise ValueError('Unified diff grew beyond the 1 MiB limit')
    text = raw.decode('utf-8', errors='strict')
    if not text.strip():
        raise ValueError('Unified diff is empty')
    return raw, {'path': config['diff_file'], 'size': len(raw),
                 'sha256': hashlib.sha256(raw).hexdigest()}


def configure_native(options, workspace, output, command, settings):
    """Use upstream parsing and metadata; never manufacture a review result."""
    from pr_agent.config_loader import _find_repository_root, get_settings
    from pr_agent.git_providers.plain_diff_provider import PlainDiffGitProvider

    options = validate(options)
    output = Path(output)
    raw, identity = read_input(workspace, options)
    (output / 'input.diff').write_bytes(raw)
    # No .git root: the upstream provider itself disables working-tree reads.
    # This directory is inside the owned container's ephemeral /tmp mount.
    empty = Path('/tmp/thruline-pr-agent-patch-only')
    empty.mkdir(exist_ok=False)
    os.chdir(empty)
    if _find_repository_root() is not None:
        raise ValueError('Plain-diff context unexpectedly has a repository root')
    settings.update({
        'config.git_provider': 'plain-diff',
        'plain_diff.content': raw.decode('utf-8'),
        'plain_diff.output_path': str(output / {'review': 'review.md', 'improve': 'improve.md', 'describe': 'description.md'}[command]),
        'plain_diff.json_output_path': str(output / 'structured-review.json'),
    })
    if command == 'describe':
        # Native default suppresses title updates for hosted PRs. A standalone
        # description has no existing hosted title, so request its native title.
        settings['pr_description.generate_ai_title'] = True
    for key, value in settings.items():
        get_settings().set(key, value)
    provider = PlainDiffGitProvider('local_diff')
    files = provider.get_diff_files()
    if not files or len(files) > 128:
        raise ValueError('Native diff parser must identify between 1 and 128 changed files')
    if any(not item.patch.strip() for item in files):
        raise ValueError('Native diff parser returned a file without a usable textual hunk; scope is incomplete')
    native_files = [{'path': item.filename, 'edit_type': str(item.edit_type),
                     'patch': item.patch, 'head_file': item.head_file,
                     'base_file': item.base_file} for item in files]
    if any(item['head_file'] or item['base_file'] for item in native_files):
        raise ValueError('Patch-only provider unexpectedly enriched source context')
    metadata = {'schema': 'thruline.pr-agent-native-diff.v1', 'mode': 'plain-diff',
                'input': identity, 'command': command,
                'provider': 'pr_agent.git_providers.plain_diff_provider.PlainDiffGitProvider',
                'working_tree_enrichment': False, 'commit_history_available': False,
                'native_files': native_files, 'native_languages': provider.get_languages(),
                'hosted_publishing': False,
                'limits': ['Only the supplied patch is reviewed; unchanged source and commit history were not supplied.',
                           'Native review text is advisory and can miss defects. No patch is applied by this provider.']}
    (output / 'native-diff-scope.json').write_text(json.dumps(metadata, indent=2) + '\n')
    return metadata
