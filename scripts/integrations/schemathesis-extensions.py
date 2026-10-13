#!/usr/bin/env python3
"""Bounded native Schemathesis extensions against an owned disposable local service."""
import argparse,base64,hashlib,json,os,re,resource,signal,subprocess,sys,tempfile,time,urllib.request
from pathlib import Path
SCHEMA='thruline.schemathesis-extensions.v1';CAPTURE='thruline.schemathesis-extensions-capture.v1'
def digest(data):return hashlib.sha256(data).hexdigest()
def file_at(root,name):
 if not isinstance(name,str)or not name or name.startswith('/')or any(x in ('','..','.')for x in name.split('/')):raise ValueError('Selected source needs a relative ordinary file path')
 p=root/name
 if any(x.is_symlink()for x in [p,*p.parents]if x!=root.parent):raise ValueError('Selected source cannot traverse symbolic links')
 if not p.is_file()or p.stat().st_size>262144:raise ValueError('Selected file missing or larger than 256 KiB')
 return p
LOADER='''import json,os,runpy\nfrom pathlib import Path\nfrom schemathesis.checks import CHECKS\nfrom schemathesis.hooks import GLOBAL_HOOK_DISPATCHER\nfrom schemathesis.specs.graphql.scalars import CUSTOM_SCALARS\nfrom schemathesis.transport.requests import REQUESTS_TRANSPORT\nbefore={'checks':set(CHECKS.get_all_names()),'hooks':set(GLOBAL_HOOK_DISPATCHER.get_all()),'scalars':set(CUSTOM_SCALARS),'serializers':set(REQUESTS_TRANSPORT._serializers)}\nrunpy.run_path(os.environ['THRULINE_EXTENSION_FILE'],run_name='thruline_accepted_extensions')\nafter={'checks':set(CHECKS.get_all_names()),'hooks':set(GLOBAL_HOOK_DISPATCHER.get_all()),'scalars':set(CUSTOM_SCALARS),'serializers':set(REQUESTS_TRANSPORT._serializers)}\ninventory=Path(os.environ['THRULINE_EXTENSION_INVENTORY']);prior=json.loads(inventory.read_text())if inventory.exists()else{}\nregistered={k:sorted(set(prior.get('registered',{}).get(k,[]))|(after[k]-before[k]))for k in before}\ninvocations=prior.get('invocations',[])+[{k:sorted(after[k]-before[k])for k in before}]\ninventory.write_text(json.dumps({'schema':'thruline.native-schemathesis-registrations.v1','registered':registered,'all':{k:sorted(after[k])for k in before},'invocations':invocations},sort_keys=True)+'\\n')\n'''
def limits():
 resource.setrlimit(resource.RLIMIT_FSIZE,(1048576,1048576));resource.setrlimit(resource.RLIMIT_CPU,(40,40))
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--root',default='/input');args=parser.parse_args();root=Path(args.root).resolve();scratch=Path(tempfile.mkdtemp(prefix='thruline-schema-'));result={'schema':CAPTURE,'runtime_version':None,'selection':None,'native':None,'native_raw_json':None,'schema_operations':[],'extension_events':[],'process':{'state':'not_started','exit_code':None},'source_files':[],'registration':None,'required_missing':[],'capture_gaps':[],'artifacts':[],'scope':{'network':'container loopback only; no live/external target','phases':['fuzzing'],'shrinking':False,'business_checks':'Only the accepted custom check functions; no general business-correctness conclusion.'}};server=None
 try:
  import schemathesis
  result['runtime_version']=schemathesis.__version__
  if schemathesis.__version__!='4.26.1':raise ValueError('Only pinned Schemathesis4.26.1 is supported')
  configfile=file_at(root,'.thruline/schemathesis.json');config=json.loads(configfile.read_text());result['selection']=config
  if set(config)!={'schema','specification','schema_file','service_file','extension_file','required_extensions','operations','seed','max_examples'}or config['schema']!=SCHEMA:raise ValueError('Unknown extensions selection contract')
  if config['specification']not in ['graphql','openapi']:raise ValueError('Only GraphQL and OpenAPI modes are implemented')
  if type(config['seed'])is not int or not 0<=config['seed']<=2147483647 or type(config['max_examples'])is not int or not 1<=config['max_examples']<=20:raise ValueError('Seed or example count outside bound')
  required=config['required_extensions']
  if set(required)!={'checks','hooks','scalars','serializers'}or any(not isinstance(v,list)or len(v)>16 or any(not isinstance(x,str)or not x or len(x)>128 for x in v)or len(v)!=len(set(v))for v in required.values()):raise ValueError('Required extension inventory is malformed')
  if not required['checks']:raise ValueError('At least one explicitly selected business check is required')
  operations=config['operations']
  if not isinstance(operations,list)or not 1<=len(operations)<=16 or len(operations)!=len(set(operations))or any(not isinstance(x,str)or not x or len(x)>128 for x in operations):raise ValueError('A bounded nonempty expected operation inventory is required')
  selected=[configfile,*[file_at(root,config[k])for k in ['schema_file','service_file','extension_file']]]
  result['source_files']=[{'path':str(p.relative_to(root)),'sha256':digest(p.read_bytes()),'bytes':p.stat().st_size}for p in selected]
  schemafile,servicefile,extensionfile=selected[1:]
  if config['specification']=='graphql':
   from graphql import build_schema
   parsed=build_schema(schemafile.read_text());result['schema_operations']=[f'{typ.name}.{name}'for typ in [parsed.query_type,parsed.mutation_type]if typ for name in typ.fields if not name.startswith('__')]
  else:
   parsed=json.loads(schemafile.read_text());result['schema_operations']=[method.upper()+' '+path for path,item in parsed.get('paths',{}).items()for method in item if method in ['get','put','post','delete','options','head','patch','trace']]
  if sorted(result['schema_operations'])!=sorted(operations):result['capture_gaps'].append('Expected operations differ from parsed schema operation inventory')
  loader=scratch/'extension-loader.py';loader.write_text(LOADER)
  env={'PATH':'/opt/python/bin:/usr/bin:/bin','HOME':str(scratch),'PYTHONDONTWRITEBYTECODE':'1','HYPOTHESIS_STORAGE_DIRECTORY':str(scratch/'hypothesis'),'SCHEMATHESIS_HOOKS':str(loader),'THRULINE_EXTENSION_FILE':str(extensionfile),'THRULINE_EXTENSION_INVENTORY':str(scratch/'registrations.json'),'THRULINE_EXTENSION_EVENTS':str(scratch/'extension-events.jsonl')}
  port=48631;url=f'http://127.0.0.1:{port}';serverargv=[sys.executable,'-B',str(servicefile),'--schema',str(schemafile),'--port',str(port),'--events',str(scratch/'service-events.jsonl')];result['service_argv']=serverargv
  with(scratch/'service.stdout').open('wb')as out,(scratch/'service.stderr').open('wb')as err:server=subprocess.Popen(serverargv,cwd=root,env=env,stdout=out,stderr=err,start_new_session=True,preexec_fn=limits)
  ready=False
  for _ in range(40):
   if server.poll()is not None:break
   try:
    with urllib.request.urlopen(url+'/ready',timeout=.25)as response:ready=response.status==200
    if ready:break
   except(OSError,ValueError):pass
   time.sleep(.05)
  if not ready:raise RuntimeError('Owned service did not become ready within the fixed bound')
  nativeargv=['/opt/python/bin/schemathesis','run',str(schemafile),'--url',url+('/graphql'if config['specification']=='graphql'else''),'--phases','fuzzing','--checks',','.join(['not_a_server_error']+([]if config['specification']=='graphql'else['status_code_conformance','response_schema_conformance'])+required['checks']),'--mode','positive','--seed',str(config['seed']),'--max-examples',str(config['max_examples']),'--workers','1','--no-shrink','--request-timeout','2','--request-retries','0','--report','json,har','--report-json-path',str(scratch/'native.json'),'--report-har-path',str(scratch/'native.har'),'--generation-database','none','--no-color'];result['native_argv']=nativeargv
  with(scratch/'native.stdout').open('wb')as out,(scratch/'native.stderr').open('wb')as err:
   try:
    run=subprocess.run(nativeargv,cwd=scratch,env=env,stdout=out,stderr=err,timeout=45,preexec_fn=limits);result['process']={'state':'completed','exit_code':run.returncode}
   except subprocess.TimeoutExpired:result['process']={'state':'timed_out','exit_code':None};result['capture_gaps'].append('Native Schemathesis exceeded45seconds')
  for filename,key in [('native.json','native'),('registrations.json','registration')]:
   p=scratch/filename
   if p.is_file()and p.stat().st_size<=1048576:
    try:
     raw=p.read_text();result[key]=json.loads(raw)
     if key=='native':result['native_raw_json']=raw
    except(ValueError,UnicodeError):result['capture_gaps'].append(filename+' is not complete JSON')
   else:result['capture_gaps'].append(filename+' was not emitted within bounds')
  events=scratch/'extension-events.jsonl'
  if events.is_file()and events.stat().st_size<=262144:
   try:result['extension_events']=[json.loads(line)for line in events.read_text().splitlines()]
   except(ValueError,UnicodeError):result['capture_gaps'].append('Extension event log is incomplete')
  for category,names in required.items():
   available=(result['registration']or{}).get('registered',{}).get(category,[])
   result['required_missing'].extend({'category':category,'name':name}for name in names if name not in available)
 except Exception as e:result['capture_gaps'].append(type(e).__name__+': '+str(e));result['process']['state']='setup_failed'if result['process']['state']=='not_started'else result['process']['state']
 finally:
  if server is not None:
   if server.poll()is None:
    os.killpg(server.pid,signal.SIGTERM)
    try:server.wait(timeout=.5)
    except subprocess.TimeoutExpired:os.killpg(server.pid,signal.SIGKILL);server.wait(timeout=.5)
   result['service_stopped']=server.poll()is not None
  else:result['service_stopped']=True
  used=0
  for p in sorted(scratch.glob('*')):
   if not p.is_file():continue
   data=p.read_bytes();original=len(data);original_digest=digest(data)
   if len(data)>262144 or used+len(data)>786432:
    result['capture_gaps'].append('Artifact exceeded capture budget: '+p.name);data=data[:max(0,min(262144,786432-used))]
   used+=len(data);result['artifacts'].append({'relative_path':p.name,'sha256':digest(data),'bytes':len(data),'original_bytes':original,'original_sha256':original_digest,'complete':len(data)==original,'base64':base64.b64encode(data).decode()})
  result['capture_complete']=not result['capture_gaps'];result['original_unchanged']=all(digest((root/r['path']).read_bytes())==r['sha256']for r in result['source_files']);print(json.dumps(result,sort_keys=True))
 return 0
if __name__=='__main__':raise SystemExit(main())
