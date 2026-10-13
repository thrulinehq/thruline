"""Revalidate actual local model assets immediately before a native invocation."""
import datetime
import hashlib
import json
from pathlib import Path
import time
import urllib.request

def _sha(path):
    h=hashlib.sha256()
    with path.open('rb') as stream:
        for data in iter(lambda:stream.read(4*1024*1024),b''):
            h.update(data)
    return h.hexdigest()

def verify(prepared_path, output, local_model_port):
    if local_model_port not in (11435, 11436):
        raise ValueError('local model identity requires an explicitly accepted loopback port')
    start=time.monotonic()
    expected=json.loads(prepared_path.read_text())
    checks=[]
    for layer in expected['layers']:
        path=Path(layer['path']); actual=_sha(path)
        checks.append({'path':str(path),'sha256':actual,'size':path.stat().st_size,
            'matched':actual==layer['verified_sha256'] and path.stat().st_size==layer['size']})
    for path,expected_sha in [(Path(expected['manifest_path']),expected['manifest_sha256']),
                              (Path(expected['runtime']['path']),expected['runtime']['sha256'])]:
        actual=_sha(path)
        checks.append({'path':str(path),'sha256':actual,'matched':actual==expected_sha})
    opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    endpoint=f'http://127.0.0.1:{local_model_port}'
    req=urllib.request.Request(endpoint+'/api/show',data=json.dumps({'model':expected['model']}).encode(),headers={'Content-Type':'application/json'})
    with opener.open(req,timeout=20) as response:
        raw=response.read(2*1024*1024+1)
    if len(raw)>2*1024*1024:
        raise ValueError('local model metadata response exceeds limit')
    (output/'model-show-captured.json').write_bytes(raw)
    show=json.loads(raw)
    checks += [{'field':'template','matched':hashlib.sha256(show['template'].encode()).hexdigest()==expected['template_sha256']},
               {'field':'system','matched':hashlib.sha256(show.get('system','').encode()).hexdigest()==expected['system_prompt_sha256']},
               {'field':'license','matched':hashlib.sha256(show['license'].encode()).hexdigest()==expected['license_sha256']},
               {'field':'model_info','matched':show['model_info']==expected['model_info']},
               {'field':'details','matched':show['details']==expected['details']}]
    with opener.open(endpoint+'/api/tags',timeout=20) as response:
        tags_raw=response.read(2*1024*1024+1)
    if len(tags_raw)>2*1024*1024:
        raise ValueError('local model tags response exceeds limit')
    (output/'model-tags-captured.json').write_bytes(tags_raw)
    selected=[item for item in json.loads(tags_raw).get('models',[]) if item.get('name')==expected['model']]
    checks.append({'field':'served_manifest','matched':len(selected)==1 and selected[0].get('digest')==expected['manifest_sha256']})
    record={'schema':'thruline.local-model-revalidation.v1','prepared_manifest_path':str(prepared_path),
            'prepared_manifest_sha256':_sha(prepared_path),'verified_at':datetime.datetime.now(datetime.UTC).isoformat(),
            'elapsed_seconds':time.monotonic()-start,'checks':checks,'matched':all(check['matched'] for check in checks)}
    (output/'model-revalidation.json').write_text(json.dumps(record,indent=2)+'\n')
    if not record['matched']:
        raise ValueError('local model identity changed; refusal before model inference')
    return record
