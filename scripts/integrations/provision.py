#!/usr/bin/env python3
"""Register acquired runtimes without running project code or downloading silently.
Runtimes remain outside the shipped Rust core. See README for rebuilding pins.
"""
import argparse, hashlib, json, pathlib, shutil, subprocess
p=argparse.ArgumentParser();p.add_argument('--root', type=pathlib.Path,default=pathlib.Path(__file__).resolve().parents[2]);p.add_argument('--out',type=pathlib.Path);p.add_argument('--isolated',action='store_true');a=p.parse_args();root=a.root.resolve();dest=a.out or root/'target/integration/runtimes/registry.json';base=root/'target/integration';tools={};missing=[]
def sha(path):return hashlib.file_digest(path.open('rb'),'sha256').hexdigest()
versions={'ast-grep':'0.45.3','cargo-deny':'0.20.2','cargo-mutants':'27.1.0','betterleaks':'95237cf8eb4d8e9f67409595b245e674832992cf','dependency-cruiser':'18.2.0','knip':'6.35.0','osv-scanner':'2.5.1'}
for name,version in versions.items():
 runtime=[];prefix=[]
 if name in ['dependency-cruiser','knip']:
  node=shutil.which('node');exe=pathlib.Path(node) if node else pathlib.Path('/missing-node');modules=base/'runtimes/javascript/node_modules';entry=modules/name/('bin/dependency-cruise.mjs' if name=='dependency-cruiser' else 'bin/knip.js');runtime=[str(modules)];prefix=[str(entry)]
  manifest=modules/name/'package.json'
  if not manifest.exists() or json.loads(manifest.read_text())['version']!=version or not entry.exists():missing.append(name);continue
 else:exe=base/('runtimes/'+name if name in ['betterleaks','osv-scanner'] else f'upstream-sources/{name}/target/release/{name}')
 if not exe.is_file():missing.append(name);continue
 if name=='cargo-mutants':
  for binary in ['cargo','rustc']:
   runtime.append(subprocess.check_output(['rustup','which',binary],text=True).strip())
  # Include compiler shared libraries, not just its executable frontend.
  lib=pathlib.Path(runtime[-1]).parent.parent/'lib';runtime.append(str(lib))
 tools[name]={'version':version,'executable':str(exe.resolve()),'sha256':sha(exe),'runtime':runtime,'prefix':prefix}
dest.parent.mkdir(parents=True,exist_ok=True);policies=dest.parent/'policies';policies.mkdir(exist_ok=True)
(policies/'no-unsafe.yml').write_bytes((root/'spec/integrations/policies/no-unsafe.yml').read_bytes())
(policies/'no-eval.yml').write_text('id: no-dynamic-eval\nlanguage: JavaScript\nrule:\n  pattern: eval($X)\nmessage: This call executes a string as code. Replace it with a parser or an explicit operation.\nseverity: warning\n')
reporters=dest.parent/'reporters';reporters.mkdir(exist_ok=True)
shutil.copyfile(root/'spec/integrations/knip-reporter.mjs',reporters/'knip.mjs')
if a.isolated:
 docker=shutil.which('docker')
 if not docker: missing.extend(['playwright','schemathesis','semgrep'])
 else:
  image=subprocess.check_output([docker,'image','inspect','thruline-verifiers:2026-09-09','--format','{{.Id}}'],text=True,timeout=10).strip()
  spec={'docker':docker,'socket':str(pathlib.Path.home()/'.docker/run/docker.sock'),'image':image,'environment':{'HOME':'/tmp','SEMGREP_ENABLE_VERSION_CHECK':'0','SEMGREP_SEND_METRICS':'off','DO_NOT_TRACK':'1'}}
  for name,version,entry in [('playwright','1.63.0','/opt/tools/node_modules/.bin/playwright'),('schemathesis','4.26.1','/opt/python/bin/python'),('semgrep','1.176.0','/opt/python/bin/semgrep')]:
   tools[name]={'version':version,'executable':docker,'sha256':sha(pathlib.Path(docker)),'runtime':[],'prefix':[entry],'container':spec}
body={'schema':'thruline.integration-runtimes.v1','tools':tools};dest.write_text(json.dumps(body,indent=2)+'\n')
print(json.dumps({'registry':str(dest),'registered':list(tools),'missing':missing,'network_used':False,'enabled_by_default':False},indent=2))
