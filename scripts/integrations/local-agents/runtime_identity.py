"""Verify the bound native runtime inventory before any agent/model execution."""
import datetime
import hashlib
import json
from pathlib import Path
import time

def sha(path):
    h=hashlib.sha256()
    with path.open('rb') as stream:
        for data in iter(lambda:stream.read(1024*1024),b''):h.update(data)
    return h.hexdigest()

def verify(runtime,output):
    started=time.monotonic()
    provenance_path=runtime/'runtime-provenance.json'
    provenance=json.loads(provenance_path.read_text())
    manifest_path=runtime/'runtime-files.json'
    if sha(manifest_path)!=provenance['files_manifest_sha256']:
        raise ValueError('runtime inventory digest changed')
    entries=json.loads(manifest_path.read_text())
    expected={entry['relative_path']:entry for entry in entries}
    actual={path.relative_to(runtime).as_posix():path for path in runtime.rglob('*') if path.is_file() and '__pycache__' not in path.parts and path.name not in ['runtime-files.json','runtime-provenance.json']}
    differences=[]
    for path in runtime.rglob('*'):
        if path.is_symlink() and not path.is_file():
            differences.append({'path':str(path),'reason':'unbound_directory_or_broken_symlink'})
    for name in sorted(set(expected)|set(actual)):
        if name not in expected or name not in actual:
            differences.append({'path':name,'reason':'unexpected' if name not in expected else 'missing'})
            continue
        path=actual[name]; entry=expected[name]
        if path.stat().st_size!=entry['size'] or sha(path)!=entry['sha256']:
            differences.append({'path':name,'reason':'bytes_changed'})
        if (str(path.resolve()) if path.is_symlink() else None)!=entry.get('symlink_target'):
            differences.append({'path':name,'reason':'symlink_target_changed'})
    python=Path(provenance['installation']['python'])
    if sha(python)!=provenance['installation']['python_sha256']:
        differences.append({'path':str(python),'reason':'python_executable_changed'})
    python_manifest=provenance['python_files_manifest']
    if sha(Path(python_manifest['path']))!=python_manifest['sha256']:
        raise ValueError('Python runtime inventory digest changed')
    python_entries=json.loads(Path(python_manifest['path']).read_text())
    python_actual={str(path):path for path in Path(python_manifest['root']).rglob('*') if path.is_file() and '__pycache__' not in path.parts}
    python_expected={entry['path']:entry for entry in python_entries}
    python_root=Path(python_manifest['root']).resolve()
    for path in python_root.rglob('*'):
        if path.is_symlink() and (not path.is_file() or python_root not in path.resolve().parents):
            differences.append({'path':str(path),'reason':'Python_runtime_unbound_symlink'})
    for name in sorted(set(python_actual)|set(python_expected)):
        if name not in python_actual or name not in python_expected:
            differences.append({'path':name,'reason':'Python_runtime_inventory_changed'})
        elif python_actual[name].stat().st_size!=python_expected[name]['size'] or sha(python_actual[name])!=python_expected[name]['sha256']:
            differences.append({'path':name,'reason':'Python_runtime_bytes_changed'})
    record={'schema':'thruline.local-agent-runtime-revalidation.v1','repository_id':provenance['repository_id'],
            'verified_at':datetime.datetime.now(datetime.UTC).isoformat(),'elapsed_seconds':time.monotonic()-started,
            'runtime_provenance_sha256':sha(provenance_path),'files_manifest_sha256':sha(manifest_path),
            'files_verified':len(entries),'python_files_verified':len(python_entries),'differences':differences,'matched':not differences}
    (output/'runtime-revalidation.json').write_text(json.dumps(record,indent=2)+'\n')
    if differences:raise ValueError('native runtime closure changed before agent execution')
    return record
