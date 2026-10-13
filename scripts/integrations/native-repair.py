#!/usr/bin/env python3
"""Hash-bound deterministic repair broker. Target source remains untrusted data.

Only selected native fixes execute in disposable stages. Fixed external oracles
measure named behavior; their files and project policies are never writable by
an analyzer or target program. The CLI retains process and command evidence.
"""
import difflib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import stat
import subprocess
import sys
import time
import uuid


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')).encode()


def write(path, value):
    temporary = path.with_name(path.name + '.new')
    with temporary.open('wb') as stream:
        stream.write(encoded(value)); stream.flush(); os.fsync(stream.fileno())
    temporary.replace(path)
    parent = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try: os.fsync(parent)
    finally: os.close(parent)


def read(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError('unsafe or oversized artifact')
    return json.loads(path.read_bytes())


def below(path, root):
    return path == root or root in path.parents


class Broker:
    def __init__(self, request, request_sha256=None):
        self.request = request
        self.root = Path(request['root']).resolve(strict=True)
        self.store = Path(request['store']).resolve(strict=True)
        self.attempt = Path(request['attempt']).resolve(strict=True)
        self.config = request['config']
        self.pack = self.config['recipe']
        self.runtime = self.config['runtime']
        self.selection = self.config['selection']
        self.commands = []
        self.capture_roots = []
        self.deadline = time.monotonic() + 600
        self.digest = request['digest']
        self.request_sha256 = request_sha256 or sha(encoded(request))
        self.persist_capture_control()

    def persist_capture_control(self):
        if len(self.capture_roots) > 128:
            raise ValueError('capture root count exceeds128')
        roots = []
        for path, kind in self.capture_roots:
            relative = path.relative_to(self.attempt)
            if len(relative.parts) != 1 or not re.fullmatch('[a-z0-9][a-z0-9-]{0,127}',relative.name) or kind not in ['native','oracle']:
                raise ValueError('invalid controlled native capture root')
            roots.append({'relative_path':relative.name,'kind':kind})
        value = {'schema':'thruline.native-repair-capture-control.v1',
                 'attempt':str(self.attempt),'root':str(self.root),'store':str(self.store),
                 'request_sha256':self.request_sha256,'broker_sha256':sha(Path(__file__).read_bytes()),
                 'roots':roots,'generation':len(roots),'capture_only_protocol':True}
        write(self.attempt/'capture-control.json',value)

    def register_capture(self, path, kind):
        self.capture_roots.append((path,kind))
        # No analyzer or oracle starts until this direct-child authority has
        # reached an atomic, fsynced control file in the protected attempt root.
        self.persist_capture_control()

    def attempt_artifacts(self):
        """Capture registered attempt outputs even when native decoding failed.

        Derived command scratch/cache trees are excluded. Native capture trees
        and named oracle outputs are explicit protocol roots, never arbitrary
        paths read from analyzer/model output. Descriptor-relative opens refuse
        symlinks in every component and bound files, traversal and total bytes.
        """
        rows, gaps, roots = [], [], []
        total = entries = 0
        limit = 8 * 1024 * 1024
        directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
        base = os.open(self.attempt, directory_flags)

        def gap(relative, reason):
            if len(gaps) < 4096:
                gaps.append({'path':str(self.attempt/relative), 'reason':str(reason)})

        def collect(parent, name, relative):
            nonlocal total
            fd = None
            try:
                fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
                metadata = os.fstat(fd)
                if not stat.S_ISREG(metadata.st_mode): raise ValueError('not a regular artifact')
                if metadata.st_size > limit: raise ValueError('artifact exceeds 8 MiB')
                if len(rows) >= 4096 or total + metadata.st_size > 64 * 1024 * 1024:
                    raise ValueError('attempt artifact count or byte budget exceeded')
                digest = hashlib.sha256(); measured = 0
                while measured <= limit:
                    chunk = os.read(fd, min(1024 * 1024, limit + 1 - measured))
                    if not chunk: break
                    measured += len(chunk); digest.update(chunk)
                after = os.fstat(fd)
                if measured != metadata.st_size or (metadata.st_size,metadata.st_mtime_ns,metadata.st_ctime_ns) != (after.st_size,after.st_mtime_ns,after.st_ctime_ns):
                    raise ValueError('artifact changed during bounded capture')
                total += measured
                rows.append({'path':str(self.attempt/relative), 'relative_path':str(relative),
                             'sha256':digest.hexdigest(), 'bytes':measured})
            except (OSError, ValueError) as error: gap(relative, error)
            finally:
                if fd is not None: os.close(fd)

        def scan(parent, relative, recursive=False, depth=0):
            nonlocal entries
            if depth > 12: gap(relative,'artifact traversal depth exceeded'); return
            try:
                with os.scandir(parent) as stream:
                    for entry in stream:
                        entries += 1
                        if entries > 8192:
                            gap(relative,'artifact traversal entry budget exceeded'); return
                        name = entry.name; child = relative/name
                        if not recursive:
                            if name in ['result.json','transport.json'] or Path(name).suffix not in ['.json','.stdout','.stderr','.sb','.cid','.py','.patch','.diff']:
                                continue
                        try:
                            metadata = entry.stat(follow_symlinks=False)
                            if stat.S_ISDIR(metadata.st_mode):
                                if recursive:
                                    nested = os.open(name,directory_flags,dir_fd=parent)
                                    try: scan(nested,child,recursive=True,depth=depth+1)
                                    finally: os.close(nested)
                            else: collect(parent,name,child)
                        except OSError as error: gap(child,error)
            except OSError as error: gap(relative,error)

        def named(relative):
            parent = os.dup(base)
            try:
                for name in relative.parts[:-1]:
                    following = os.open(name,directory_flags,dir_fd=parent)
                    os.close(parent); parent = following
                os.stat(relative.name,dir_fd=parent,follow_symlinks=False)
                collect(parent,relative.name,relative)
            except FileNotFoundError:
                pass  # Optional installed entry points do not exist before npm runs.
            except OSError as error: gap(relative,error)
            finally: os.close(parent)

        try:
            scan(base,Path())
            for path,kind in self.capture_roots:
                relative = path.relative_to(self.attempt)
                roots.append({'path':str(path),'relative_path':str(relative),'kind':kind})
                parent = None
                try:
                    if len(relative.parts) != 1: raise ValueError('capture root must be a direct attempt child')
                    parent = os.open(relative.name,directory_flags,dir_fd=base)
                    scan(parent,relative,recursive=kind=='native')
                    if kind == 'oracle':
                        for name in ['package.json','package-lock.json','index.js','node_modules/minimist/package.json','node_modules/minimist/index.js']:
                            named(relative/'consumer'/name)
                except (OSError,ValueError) as error: gap(relative,error)
                finally:
                    if parent is not None: os.close(parent)
        finally: os.close(base)
        return {'schema':'thruline.native-repair-attempt-artifacts.v1','complete':not gaps,
                'roots':roots,'artifacts':sorted(rows,key=lambda row:row['relative_path']),'gaps':gaps,
                'scope':'Current attempt command/control files, registered native capture trees and named oracle outputs. Derived command scratch and npm cache trees excluded; empty roots means no native/oracle invocation reached its capture stage.'}

    def run(self, argv, cwd, label, writable=(), sandbox=True, extra_env=None, python_source_only=False):
        label = f'{len(self.commands):03d}-{label}'
        scratch = self.attempt / (label + '.scratch'); scratch.mkdir()
        outpath, errpath = self.attempt / (label + '.stdout'), self.attempt / (label + '.stderr')
        env = {'PATH':'/usr/bin:/bin', 'HOME':str(scratch), 'TMPDIR':str(scratch),
               'PYTHONNOUSERSITE':'1', 'PYTHONDONTWRITEBYTECODE':'1', 'CARGO_NET_OFFLINE':'true'}
        env.update(extra_env or {})
        actual = list(argv)
        profile = None
        if sandbox:
            if sys.platform != 'darwin': raise ValueError('native repair child sandbox unavailable on this host')
            profile = '(version 1)(allow default)(deny network*)(deny file-read*)(deny file-write*)(allow file-read-metadata)(allow file-read* (literal "/"))'
            reads = [cwd, scratch, Path(self.config['python']['path']), Path(self.selection['oracle']['path']),
                     Path(self.runtime['executable']), *[Path(p) for p in self.runtime['runtime']]]
            if self.pack == 'osv-scanner':
                reads.extend(Path(p) for p in self.selection['guided']['consumer_runtime'])
                reads.append(Path(self.selection['guided']['data']['root']))
            system = ['/System', '/usr/bin', '/usr/lib', '/usr/share', '/bin', '/sbin', '/dev',
                      '/Applications/Xcode.app', '/Library/Frameworks/Python.framework',
                      '/opt/homebrew/Cellar', '/opt/homebrew/opt', '/opt/homebrew/lib', '/opt/homebrew/share']
            reads.extend(Path(p) for p in system if Path(p).exists())
            reads.extend(Path(p) for p in ['/private/etc/localtime', '/private/etc/passwd', '/private/etc/group',
                         '/private/etc/services', '/private/etc/protocols', '/private/etc/ssl/openssl.cnf'] if Path(p).exists())
            for path in reads:
                path = path.resolve(strict=True)
                kind = 'subpath' if path.is_dir() else 'literal'
                profile += f'(allow file-read* ({kind} {json.dumps(str(path))}))'
                for parent in path.parents:
                    profile += f'(allow file-read* (literal {json.dumps(str(parent))}))'
            profile += f'(allow file-write* (subpath {json.dumps(str(scratch))}) (literal "/dev/null"))'
            for path in writable:
                path = Path(path).resolve(strict=True)
                kind = 'subpath' if path.is_dir() else 'literal'
                profile += f'(allow file-read* ({kind} {json.dumps(str(path))}))(allow file-write* ({kind} {json.dumps(str(path))}))'
            if python_source_only:
                # -I ignores Python environment flags, and a child interpreter
                # need not inherit its parent's -X option. Refuse target bytecode
                # reads in the inherited oracle sandbox as well as redirecting
                # the direct oracle interpreter's standard cache lookup.
                cache_pattern = '^' + re.escape(str(Path(cwd).resolve(strict=True))) + r'/.*\.py[co]$'
                profile += f'(deny file-read-data (regex {json.dumps(cache_pattern)}))'
            (self.attempt / (label + '.sb')).write_text(profile)
            actual = ['/usr/bin/sandbox-exec', '-p', profile, '--', *actual]
        started = time.monotonic()
        with outpath.open('wb') as out, errpath.open('wb') as err:
            child = subprocess.Popen(actual, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                stdout=out, stderr=err, preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_CPU, (120, 120)))
            while child.poll() is None:
                if time.monotonic() > self.deadline or time.monotonic() - started > 150:
                    child.kill(); child.wait(); raise TimeoutError(label + ' timed out')
                if outpath.stat().st_size + errpath.stat().st_size > 8 * 1024 * 1024:
                    child.kill(); child.wait(); raise ValueError(label + ' exceeded native output budget')
                time.sleep(.02)
        if outpath.stat().st_size + errpath.stat().st_size > 8 * 1024 * 1024:
            raise ValueError(label + ' exceeded output budget at exit')
        row = {'label':label, 'argv':argv, 'cwd':str(cwd), 'exit_code':child.returncode,
               'seconds':time.monotonic()-started, 'cpu_seconds':120,
               'sandbox':'macos-seatbelt-no-network-selected-writes' if sandbox else 'trusted fixed local broker operation',
               'stdout':str(outpath), 'stderr':str(errpath), 'stdout_sha256':sha(outpath.read_bytes()),
               'stderr_sha256':sha(errpath.read_bytes())}
        if profile is not None: row['profile_sha256'] = sha(profile.encode())
        self.commands.append(row); write(self.attempt / 'commands.json', self.commands)
        return child.returncode, outpath.read_bytes(), errpath.read_bytes()

    def git(self, args, cwd=None, label='git'):
        code, out, err = self.run(['/usr/bin/git', '-c', 'core.hooksPath=/dev/null', '-c', 'submodule.recurse=false',
            '-c', 'core.fsmonitor=false', '-c', 'diff.external=', '-c', 'core.autocrlf=false', *args],
            cwd or self.root, label, sandbox=False)
        if code: raise ValueError('Fixed Git operation failed: ' + err.decode(errors='replace')[:1000])
        return out

    def snapshot(self, root, label):
        names = self.git(['ls-files', '--cached', '--others', '--exclude-standard', '-z'], root, label).split(b'\0')
        names = sorted(set(n.decode() for n in names if n))
        if len(names) > 10000: raise ValueError('source exceeds 10,000 selected files')
        files, total = {}, 0
        for name in names:
            rel = Path(name); path = root / rel
            if rel.is_absolute() or '..' in rel.parts or '.git' in rel.parts: raise ValueError('invalid source path')
            if path.is_symlink() or any(p.is_symlink() for p in path.parents if below(p, root)):
                raise ValueError('symlink source is outside supported repair scope: ' + name)
            if not path.exists(): files[name] = None; continue
            if not path.is_file(): raise ValueError('submodule or special source needs another adapter')
            size = path.stat().st_size; total += size
            if size > 8 * 1024 * 1024 or total > 64 * 1024 * 1024: raise ValueError('source byte limit exceeded')
            files[name] = {'sha256':sha(path.read_bytes()), 'mode':path.stat().st_mode & 0o777}
        return {'files':files, 'bytes':total, 'scope':'tracked and nonignored untracked files; ignored inputs excluded'}

    def configured(self):
        versions = {'clippy':'0.1.97', 'knip':'6.35.0', 'openrewrite':'8.90.4', 'semgrep':'1.176.0', 'opengrep':'1.30.0', 'osv-scanner':'2.5.1'}
        if self.config.get('schema') != 'thruline.native-repair-config.v1' or self.pack not in versions:
            raise ValueError('unsupported native repair configuration')
        if self.runtime['version'] != versions[self.pack]: raise ValueError('unsupported native repair runtime')
        if set(self.selection) - ({'editable_files', 'oracle', 'guided'} if self.pack == 'osv-scanner' else {'editable_files', 'oracle', 'rule_file'} if self.pack in ['semgrep', 'opengrep'] else {'editable_files', 'oracle', 'fix_container'}): raise ValueError('unknown repair selection field')
        selected = self.selection['editable_files']
        if not isinstance(selected, list) or not 1 <= len(selected) <= 128 or len(set(selected)) != len(selected):
            raise ValueError('choose 1-128 distinct editable source files')
        extensions = {'clippy':{'.rs'}, 'knip':{'.js','.mjs','.ts','.tsx'}, 'openrewrite':{'.java'}, 'semgrep':{'.py'}, 'opengrep':{'.py'}, 'osv-scanner':{'.json'}}
        properties_recipe = False
        if self.pack == 'openrewrite':
            recipe = read(self.root / '.thruline/openrewrite.json')
            properties_recipe = recipe.get('schema') == 'thruline.openrewrite-properties.v1'
            if properties_recipe:
                if recipe.get('recipe') != 'org.openrewrite.properties.ChangePropertyValue':
                    raise ValueError('Only the selected native exact-value Properties recipe is supported')
                extensions['openrewrite'] = {'.properties'}
        for name in selected:
            path = Path(name)
            if properties_recipe and '.thruline' in path.parts:
                raise ValueError('Properties repair cannot edit product policy files')
            if path.is_absolute() or '..' in path.parts or '.git' in path.parts or path.suffix not in extensions[self.pack]:
                raise ValueError('editable scope cannot include policy, test configuration, manifests, or unrecognized source')
        for item in [self.config['python'], self.selection['oracle']]:
            path = Path(item['path']).resolve(strict=True)
            if below(path, self.root) or below(path, self.store): raise ValueError('oracle/runtime must be external to source and repair store')
            if path.stat().st_size > 1024**3 or sha(path.read_bytes()) != item['sha256']: raise ValueError('pinned oracle/runtime changed')
        oracle = self.selection['oracle']
        if set(oracle) != {'path','sha256','baseline_case_ids','changed_case_ids'}: raise ValueError('unrecognized independent oracle contract')
        for phase in ['baseline', 'changed']:
            ids = oracle[phase + '_case_ids']
            if not isinstance(ids,list) or not ids or len(ids)>1000 or len(ids)!=len(set(ids)) or any(not isinstance(x,str) or not x for x in ids):
                raise ValueError('independent case identifiers must be explicit and distinct')

        if self.pack == 'osv-scanner':self.osv_configuration()

        if self.pack in ['semgrep', 'opengrep']:
            rule = self.selection.get('rule_file', {})
            if set(rule) != {'path', 'sha256', 'rule_id'} or not re.fullmatch('[A-Za-z0-9_.-]{1,100}', rule['rule_id']):
                raise ValueError(self.pack.title()+' repair requires one pinned external rule identity')
            path = Path(rule['path'])
            if not path.is_absolute() or path.is_symlink() or below(path.resolve(strict=True), self.root) or below(path.resolve(), self.store):
                raise ValueError('Reviewed rule must be external and immutable to source and repair stage')
            if path.stat().st_size > 65536 or sha(path.read_bytes()) != rule['sha256']:
                raise ValueError('Reviewed '+self.pack.title()+' rule changed')
            rules = read(path)
            if set(rules) != {'rules'} or not isinstance(rules['rules'], list) or len(rules['rules']) != 1:
                raise ValueError('Only one explicitly selected native fix rule is supported')
            item = rules['rules'][0]
            if set(item) != {'id','languages','severity','message','pattern','fix'} or item['id'] != rule['rule_id'] or item['languages'] != ['python'] or item['severity'] not in ['ERROR','WARNING','INFO'] or any(not isinstance(item[k],str) or not item[k] for k in ['message','pattern','fix']):
                raise ValueError('Only a reviewed Python pattern and literal native fix is supported; regex, taint and remote rules are withheld')

    def oracle(self, stage, phase):
        before = self.snapshot(stage, phase+'-oracle-source-before')
        scratch = self.attempt / (phase+'-oracle-output'); scratch.mkdir()
        self.register_capture(scratch, 'oracle')
        oracle = self.selection['oracle']
        code, out, err = self.run([self.config['python']['path'], '-I', '-B', '-X', 'pycache_prefix=/dev/null', oracle['path'], '--root', str(stage),
            '--phase', phase, '--scratch', str(scratch)], stage, phase+'-oracle', writable=[scratch], python_source_only=True)
        self.configured()
        if self.snapshot(stage, phase+'-oracle-source-after') != before: raise ValueError('oracle/target changed the candidate')
        try: result = json.loads(out)
        except (ValueError,UnicodeDecodeError):
            return {'passed':False,'exit_code':code,'report':None,'error':'Independent oracle returned no valid JSON report','stderr':err.decode(errors='replace')[:4000],'oracle_sha256':oracle['sha256'],'source_sha256':sha(encoded(before))}
        cases = result.get('cases', []); ids = oracle[phase+'_case_ids']
        valid = result.get('schema') == 'thruline.repair-oracle.v1' and len(cases)==len(ids) and sorted(c.get('id','') for c in cases)==sorted(ids)
        if not valid or result.get('total') != len(ids) or type(result.get('passed')) is not int or result.get('passed') != sum(c.get('status')=='passed' for c in cases) or result.get('failed') != sum(c.get('status')!='passed' for c in cases):
            raise ValueError('oracle identity/count response differs from frozen acceptance cases')
        measured = {'passed':code==0 and all(c.get('status')=='passed' for c in cases) and not err,
                    'exit_code':code, 'report':result, 'oracle_sha256':oracle['sha256'], 'source_sha256':sha(encoded(before))}
        if self.pack == 'osv-scanner':
            executed = scratch/'executed-oracle.py'
            executed.write_bytes(Path(oracle['path']).read_bytes())
            if sha(executed.read_bytes()) != oracle['sha256']:raise ValueError('Executed independent oracle bytes changed')
            paths = [p for p in scratch.iterdir() if p.is_file() and p.suffix in ['.json','.stdout','.stderr','.py']]
            for name in ['package.json','package-lock.json','index.js','node_modules/minimist/package.json','node_modules/minimist/index.js']:
                item = scratch/'consumer'/name
                if item.is_file():paths.append(item)
            artifacts = [{'path':str(p),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for p in sorted(paths)]
            measured['native'] = {'schema':'thruline.repair-oracle-capture.v1','phase':phase,
                                  'artifacts':artifacts,'scope':'Actual offline npm install and named consumer command outputs; exact executed oracle and installed dependency entry points retained.'}
        return measured

    def knip_scope(self, stage):
        # Executable configuration can execute inside the analyzer process and
        # spoof its report. This adapter supports a static, dependency-free ESM
        # scope only; project/plugin configuration needs a separate qualification.
        package = read(stage/'package.json'); config = read(stage/'knip.json')
        if package.get('private') is not True or package.get('type') != 'module':
            raise ValueError('Knip repair requires an explicitly private ESM package')
        if any(package.get(key) for key in ['scripts','dependencies','devDependencies','peerDependencies','optionalDependencies','workspaces','knip']):
            raise ValueError('Knip repair with executable package/plugin configuration is not qualified')
        if set(config) != {'entry','project'} or any(not isinstance(config[k],list) or not config[k] or any(not isinstance(x,str) or x.startswith('/') or '..' in Path(x).parts for x in config[k]) for k in config):
            raise ValueError('Knip repair requires static local entry/project arrays')
        for name in self.snapshot(stage,'knip-static-scope')['files']:
            if name not in ['package.json','knip.json'] and Path(name).suffix not in ['.js','.mjs']:
                raise ValueError('Knip repair currently supports plain ESM source and static JSON configuration only')
            if re.search(r'(?:^|/)(?:knip|vite|vitest|webpack|rollup|jest|babel|eslint|svelte|next|nuxt)\.config\.',name) or Path(name).name.startswith('knip.config.'):
                raise ValueError('Executable analyzer/plugin configuration is withheld')

    def native(self, stage, label, fix=False):
        runtime = self.runtime; output = self.attempt / (label+'-native'); output.mkdir()
        self.register_capture(output, 'native')
        argv = [runtime['executable'], *runtime['prefix']]
        if self.pack == 'osv-scanner':
            return self.osv_native(stage, output, label, fix)
        if self.pack == 'semgrep':
            return self.semgrep_native(stage, output, label, fix)
        if self.pack == 'opengrep':
            return self.opengrep_native(stage, output, label, fix)
        if self.pack == 'knip':
            self.knip_scope(stage)
            argv += ['--config',str(stage/'knip.json')]
            argv += ['--reporter','json','--no-progress','--no-config-hints','--include','exports']
            if fix: argv += ['--fix','--fix-type','exports']
            code, out, err = self.run(argv, stage, label, writable=[stage/name for name in self.selection['editable_files']] if fix else [])
            value = json.loads(out)
            if code not in [0,1] or not isinstance(value,dict) or not isinstance(value.get('issues'),list): raise ValueError('Knip did not return a complete native issue report')
            issues = [{'file':item['file'], **issue} for item in value['issues'] for issue in item.get('exports',[])]
            return {'complete':True, 'count':len(issues), 'issues':issues, 'native':value}
        if self.pack == 'clippy' and fix:
            return self.cargo_fix(stage, output, label)
        argv.append(str(output))
        if fix: argv.append('--apply-isolated')
        code, out, err = self.run(argv, stage, label, writable=[output])
        value = json.loads(out)
        if code != 0 or value.get('exit_code') != 0 or value.get('original_unchanged') is not True:
            raise ValueError('Native detector/recipe did not complete successfully')
        if self.pack == 'clippy':
            if value.get('schema') not in ('thruline.clippy-capture.v1','thruline.clippy-capture.v2') or value.get('parse_error_lines') or value.get('metadata_exit') != 0:
                raise ValueError('Incomplete Clippy compiler capture')
            cfg=read(stage/'.thruline/clippy.json')
            lint='clippy::unnecessary_map_or' if cfg.get('schema')=='thruline.clippy.v2' else 'clippy::needless_return'
            if cfg.get('lints') != [lint] or (cfg.get('schema')=='thruline.clippy.v2' and (value.get('policy_unchanged') is not True or value.get('msrv_scope',{}).get('conflicts') or cfg.get('suppression')!='force-warn')):
                raise ValueError('Clippy repair requires an unchanged, nonconflicting force-warn policy')
            issues = [event['message'] for event in value['events'] if event.get('reason')=='compiler-message' and (event.get('message',{}).get('code') or {}).get('code')==lint]
        else:
            native = value.get('native_report')
            properties_recipe = value.get('schema') == 'thruline.openrewrite-module-capture.v1'
            if value.get('schema') not in ('thruline.openrewrite-capture.v1','thruline.openrewrite-module-capture.v1') or value.get('errors') or not native or native.get('errors') or sorted(native.get('parsed_files',[])) != sorted(value['selected_files']):
                raise ValueError('Incomplete named native recipe capture')
            if properties_recipe and (native.get('schema') != 'thruline.openrewrite-properties-native.v1' or native.get('recipe') != 'org.openrewrite.properties.ChangePropertyValue' or native.get('fixed_point_changes') != 0 or not native.get('matching_key_count') or native.get('unexpected_value_count') != 0 or sorted(value.get('discovered_files',[])) != sorted(value['selected_files']) or value.get('symlinks_not_followed')):
                raise ValueError('Incomplete native Properties recipe scope')
            issues = native['changes']
            if fix:
                for change in issues:
                    name = change['path']
                    if name not in self.selection['editable_files']: raise ValueError('Recipe touched unselected source')
                    proposed = Path(value['isolated_source']) / name
                    if proposed.read_text() != change['after'] or (stage/name).read_text() != change['before']:
                        raise ValueError('Recipe source/diff identity mismatch')
                    (stage/name).write_bytes(proposed.read_bytes())
        return {'complete':True, 'count':len(issues), 'issues':issues, 'native':value}

    def osv_configuration(self):
        guided = self.selection.get('guided', {})
        if set(guided) != {'image_id','docker','linux_executable','driver','data','consumer_runtime'} or not re.fullmatch('sha256:[0-9a-f]{64}',guided['image_id']):
            raise ValueError('A pinned offline native OSV environment is required')
        consumer = guided['consumer_runtime']
        if not isinstance(consumer,list) or not 1<=len(consumer)<=4 or any(not Path(p).is_absolute() or below(Path(p).resolve(strict=True),self.root) or below(Path(p).resolve(),self.store) for p in consumer):
            raise ValueError('Pinned consumer runtime paths must be outside source and repair store')
        if self.selection['editable_files'] != ['package-lock.json']:
            raise ValueError('Guided npm in-place remediation can change only package-lock.json')
        for key in ['docker','linux_executable','driver']:
            item = guided[key]; path = Path(item['path'])
            if set(item) != {'path','sha256'} or not path.is_absolute() or path.is_symlink() or below(path.resolve(strict=True),self.root) or below(path.resolve(),self.store) or sha(path.read_bytes()) != item['sha256']:
                raise ValueError('Pinned native OSV driver/runtime is unavailable or changed')
        data = guided['data']; root = Path(data['root'])
        if set(data) != {'root','files','routes','packages','scope'} or not isinstance(data['scope'],str) or not data['scope'] or len(data['scope']) > 4000:
            raise ValueError('The offline advisory and metadata coverage must be explicit')
        if not root.is_absolute() or root.is_symlink() or below(root.resolve(strict=True),self.root) or below(root.resolve(),self.store):
            raise ValueError('Offline metadata must remain outside source and the repair store')
        if not isinstance(data['packages'],list) or not 1 <= len(data['packages']) <= 16 or len(set(data['packages'])) != len(data['packages']) or any(not re.fullmatch('[a-z0-9][a-z0-9._-]{0,127}',x) for x in data['packages']):
            raise ValueError('Only explicitly named unscoped npm packages are supported')
        if not isinstance(data['files'],list) or not 1 <= len(data['files']) <= 128:
            raise ValueError('Offline data files must be a finite pinned inventory')
        names = set(); total = 0
        for item in data['files']:
            if set(item) != {'path','sha256','bytes'}:raise ValueError('Invalid offline data identity')
            name = item['path']; rel = Path(name); path = root/rel
            if rel.is_absolute() or '..' in rel.parts or name in names or path.is_symlink() or any(p.is_symlink() for p in path.parents if below(p,root)):
                raise ValueError('Offline data traversal, duplicate or symlink refused')
            names.add(name); total += path.stat().st_size
            if path.stat().st_size != item['bytes'] or item['bytes'] > 8*1024**2 or total > 32*1024**2 or sha(path.read_bytes()) != item['sha256']:
                raise ValueError('Offline metadata/advisory bytes changed or exceed scope')
        if 'database/osv-scalibr/npm/all.zip' not in names:raise ValueError('Pinned npm advisory database is required')
        if not isinstance(data['routes'],dict) or not data['routes'] or len(data['routes']) > 100:
            raise ValueError('Finite native metadata endpoints must be selected')
        by_name = {row['path']:row for row in data['files']}
        for route,item in data['routes'].items():
            if not isinstance(route,str) or not re.fullmatch('/[a-z0-9][a-z0-9._-]*(?:/[0-9][0-9.A-Za-z+-]*)?',route) or route.split('/')[1] not in data['packages'] or set(item) != {'path','sha256'} or item['path'] not in names or by_name[item['path']]['sha256'] != item['sha256']:
                raise ValueError('Metadata endpoint is outside the frozen package inventory')
        for package in data['packages']:
            if '/'+package not in data['routes']:raise ValueError('Full original package metadata is required')
        return guided

    def osv_scope(self, stage):
        guided = self.osv_configuration(); data = guided['data']
        package = read(stage/'package.json'); lock = read(stage/'package-lock.json')
        if package.get('private') is not True or any(package.get(k) for k in ['scripts','workspaces','overrides','resolutions','devDependencies','peerDependencies','optionalDependencies']):
            raise ValueError('Guided repair currently requires a private package with simple direct dependencies and no package scripts')
        dependencies = package.get('dependencies', {})
        if not dependencies or set(dependencies) - set(data['packages']) or lock.get('lockfileVersion') != 3:
            raise ValueError('Selected dependency graph is outside the explicit offline npm scope')
        packages = lock.get('packages', {})
        if set(packages) != {'', *['node_modules/'+name for name in dependencies]} or packages[''].get('dependencies') != dependencies:
            raise ValueError('Only a flat complete lockfile graph matching the unchanged manifest is supported')
        for name in dependencies:
            item = packages['node_modules/'+name]
            if not isinstance(dependencies[name],str) or not re.fullmatch(r'\^?[0-9]+\.[0-9]+\.[0-9]+',dependencies[name]):
                raise ValueError('Unsupported native npm dependency range')
            route = data['routes']['/'+name]; original = read(Path(data['root'])/route['path'])
            native = original.get('versions',{}).get(item.get('version'), {})
            dist = native.get('dist', {})
            if native.get('name') != name or item.get('resolved') != dist.get('tarball') or item.get('integrity') != dist.get('integrity') or not str(item.get('resolved','')).startswith('https://registry.npmjs.org/'+name+'/-/') or native.get('dependencies') or native.get('optionalDependencies'):
                raise ValueError('Lockfile package identity or transitive graph differs from retained publisher metadata')
        if any((stage/name).exists() for name in ['.npmrc','npm-shrinkwrap.json']):
            raise ValueError('Project registry overrides and competing lockfiles need separately qualified scope')

    def osv_native(self, stage, output, label, fix):
        self.configured(); self.osv_scope(stage)
        guided = self.selection['guided']; data = guided['data']
        captured = output/'inputs'; captured.mkdir()
        for row in data['files']:
            target = captured/row['path']; target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(Path(data['root'])/row['path'],target)
        driver = output/'driver.py'; shutil.copyfile(guided['driver']['path'],driver)
        policy = output/'policy.json'; write(policy,{'routes':data['routes'],'scope':data['scope']})
        work = stage
        if not fix:
            # Native guidance is executed on an isolated copy to retain the exact
            # native proposed lock bytes before a second selected-file application.
            work = output/'proposal'; work.mkdir()
            for name in ['package.json','package-lock.json']:shutil.copy2(stage/name,work/name)
        native_output = output/'native'; native_output.mkdir()
        docker = guided['docker']['path']; image = guided['image_id']
        code,out,_ = self.run([docker,'image','inspect',image],self.attempt,label+'-image',sandbox=False)
        images=json.loads(out)
        if code or len(images)!=1 or images[0]['Id']!=image:raise ValueError('Pinned OSV container image changed')
        mounts=[(work,'/source',False),(work/'package-lock.json','/source/package-lock.json',True),
                (captured,'/data',False),(driver,'/driver.py',False),(policy,'/policy.json',False),
                (Path(guided['linux_executable']['path']),'/osv-scanner',False),(native_output,'/output',True)]
        if any(',' in str(path) for path,_,_ in mounts):raise ValueError('Comma-containing bind paths need another native adapter')
        cidfile=self.attempt/(label+'-osv.cid')
        argv=[docker,'create','--cidfile',str(cidfile),'--label','org.thruline.native-repair-attempt='+sha(str(self.attempt).encode()),
              '--network','none','--read-only','--cpus','2','--memory','768m','--pids-limit','96','--cap-drop','ALL',
              '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}',
              '--tmpfs','/tmp:rw,nosuid,nodev,size=64m','--workdir','/source']
        for source,dest,rw in mounts:argv.extend(['--mount','type=bind,src='+str(source)+',dst='+dest+('' if rw else ',readonly')])
        argv.extend([image,'python3','-I','/driver.py'])
        created,_,_=self.run(argv,self.attempt,label+'-create',sandbox=False)
        cid=cidfile.read_text().strip()
        if created or not re.fullmatch('[0-9a-f]{64}',cid):raise ValueError('OSV container identity was not captured before execution')
        code,out,err=self.run([docker,'start','--attach',cid],self.attempt,label+'-run',sandbox=False)
        inspected,state_out,_=self.run([docker,'inspect',cid],self.attempt,label+'-state',sandbox=False)
        state=json.loads(state_out)[0]; host=state['HostConfig']
        actual=sorted((r['Source'],r['Destination'],r['RW']) for r in state['Mounts'])
        expected=sorted((str(p),d,rw) for p,d,rw in mounts)
        if inspected or state['Image']!=image or host['NetworkMode']!='none' or not host['ReadonlyRootfs'] or host['Memory']!=768*1024**2 or host['PidsLimit']!=96 or host['NanoCpus']!=2000000000 or host['CapDrop']!=['ALL'] or 'no-new-privileges' not in host['SecurityOpt'] or actual!=expected or state['State']['Running'] or state['State']['OOMKilled'] or state['State']['Pid']!=0:
            raise ValueError('Native OSV environment differs from the reviewed scope')
        self.run([docker,'rm','-f',cid],self.attempt,label+'-remove',sandbox=False)
        value=json.loads(out)
        if code or state['State']['ExitCode']!=0 or value.get('exit_code')!=0 or value.get('all_metadata_requests_in_scope') is not True:
            raise ValueError('Native OSV did not complete within its retained metadata scope')
        report=value.get('native_report',{})
        if report.get('path')!='/source/package-lock.json' or report.get('ecosystem')!='npm' or report.get('strategy')!='in-place' or 'vulnerabilities' not in report or 'patches' not in report or not (report['vulnerabilities'] is None or isinstance(report['vulnerabilities'],list)) or not (report['patches'] is None or isinstance(report['patches'],list)):
            raise ValueError('Native guided remediation report has an unsupported shape')
        if report['vulnerabilities'] is None and (report['patches'] is not None or value['before_sha256'] != value['after_sha256']):
            raise ValueError('Native empty-guidance output conflicts with patch or source facts')
        issues=report['vulnerabilities'] if report['vulnerabilities'] is not None else []
        proposed=(work/'package-lock.json').read_bytes()
        if sha(proposed)!=value.get('after_sha256'):raise ValueError('Native proposed lockfile capture differs')
        self.osv_scope(work); self.configured()
        for row in data['files']:
            if sha((captured/row['path']).read_bytes())!=row['sha256']:raise ValueError('Captured native data changed')
        artifact_paths=[p for p in output.rglob('*') if p.is_file() and not p.is_symlink()]
        artifact_paths += [p for p in self.attempt.glob('*') if p.is_file() and label in p.name and p.suffix in ['.stdout','.stderr','.json','.cid']]
        artifacts=[{'path':str(p),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for p in sorted(set(artifact_paths))]
        envelope={'schema':'thruline.osv-guided-repair-capture.v1','version':'2.5.1','native':value,'container_id':cid,'container_state':state,
                  'linux_executable':guided['linux_executable'],'driver':guided['driver'],'data_scope':data['scope'],'artifacts':artifacts,
                  'scope':'Experimental native in-place npm lockfile guidance over the explicitly retained flat package graph and dated advisory/metadata subset; independent consumer checks remain required.'}
        return {'complete':True,'count':len(issues),'issues':issues,'proposed_lock':proposed.decode(),'native':envelope}


    def opengrep_native(self, stage, output, label, fix):
        """Execute the pinned native fixer; independently compare its exact spans.

        No project config discovery, remote rules, experimental CLI or target
        commands are enabled. The shared broker confines source writes and
        captures every registered native byte on success and on refusal.
        """
        self.configured()
        runtime = self.runtime
        executable = Path(runtime['executable']).resolve(strict=True)
        if executable.stat().st_size > 128 * 1024 * 1024 or sha(executable.read_bytes()) != runtime['sha256']:
            raise ValueError('Pinned Opengrep executable changed')
        caches = [Path(path) for path in runtime['runtime'] if Path(path).name == 'cache']
        if len(caches) != 1 or not caches[0].is_dir() or caches[0].is_symlink() or below(caches[0].resolve(), self.root) or below(caches[0].resolve(), self.store):
            raise ValueError('Opengrep needs its one provisioned read-only native cache outside source and repair store')
        rule = output/'reviewed-rule.json'
        rule.write_bytes(Path(self.selection['rule_file']['path']).read_bytes())
        if sha(rule.read_bytes()) != self.selection['rule_file']['sha256']:
            raise ValueError('Captured Opengrep rule changed')
        before = {name:(stage/name).read_bytes() for name in self.selection['editable_files']}
        reports = []
        for phase in (['dryrun','apply'] if fix else ['scan']):
            sarif = output/(phase+'.sarif')
            argv = [str(executable), *runtime['prefix'], 'scan', '--config', str(rule),
                    '--json', '--sarif-output', str(sarif), '--dataflow-traces',
                    '--taint-intrafile', '--no-rewrite-rule-ids', '--disable-version-check',
                    '--disable-nosem', '--no-git-ignore', '--strict', '--error',
                    '--timeout', '20', '--max-memory', '2048']
            if phase != 'scan': argv.append('--autofix')
            if phase == 'dryrun': argv.append('--dryrun')
            argv.extend(self.selection['editable_files'])
            env = {'XDG_CACHE_HOME':str(caches[0]), 'SEMGREP_LOG_FILE':str(output/(phase+'.log')),
                   'SEMGREP_SETTINGS_FILE':str(output/(phase+'-settings.yml')),
                   'SEMGREP_ENABLE_VERSION_CHECK':'0', 'SEMGREP_SEND_METRICS':'off', 'DO_NOT_TRACK':'1'}
            writable = [output] + ([stage/name for name in self.selection['editable_files']] if phase == 'apply' else [])
            code, out, err = self.run(argv, stage, label+'-'+phase, writable=writable, extra_env=env)
            (output/(phase+'.json')).write_bytes(out)
            (output/(phase+'.stderr')).write_bytes(err)
            value = json.loads(out)
            if code not in [0,1] or value.get('version') != '1.30.0' or not isinstance(value.get('results'),list) or value.get('errors') != [] or value.get('skipped_rules') != [] or value.get('paths',{}).get('skipped') or sorted(value.get('paths',{}).get('scanned',[])) != sorted(self.selection['editable_files']):
                raise ValueError('Opengrep did not complete every selected file and rule')
            if (code == 1) != bool(value['results']):
                raise ValueError('Opengrep finding exit differs from its native report')
            if any(row.get('check_id') != self.selection['rule_file']['rule_id'] or row.get('path') not in self.selection['editable_files'] or row.get('extra',{}).get('is_ignored') is True for row in value['results']):
                raise ValueError('Opengrep returned a suppressed or out-of-scope fix')
            structured = read(sarif)
            if structured.get('version') != '2.1.0' or len(structured.get('runs',[])) != 1 or len(structured['runs'][0].get('results',[])) != len(value['results']):
                raise ValueError('Native Opengrep JSON and SARIF result coverage differ')
            if phase != 'apply' and before != {name:(stage/name).read_bytes() for name in self.selection['editable_files']}:
                raise ValueError('Opengrep changed source during a read-only scan or dryrun')
            reports.append({'phase':phase,'argv':argv,'exit_code':code,'report':value,'sarif_results':len(structured['runs'][0]['results'])})
        self.configured()
        paths = list(output.iterdir())
        if len(paths) > 128 or any(path.is_symlink() or not path.is_file() or path.stat().st_size > 8 * 1024 * 1024 for path in paths) or sum(path.stat().st_size for path in paths) > 64 * 1024 * 1024:
            raise ValueError('Native Opengrep artifact tree is not a bounded regular-file closure')
        artifacts = [{'path':str(path),'sha256':sha(path.read_bytes()),'bytes':path.stat().st_size} for path in sorted(paths)]
        value = reports[0]['report']
        native = {'schema':'thruline.opengrep-repair-capture.v1','engine':'opengrep','version':'1.30.0',
                  'phases':reports,'artifacts':artifacts,'selected_files':self.selection['editable_files'],
                  'rule_sha256':self.selection['rule_file']['sha256'],
                  'scope':'One pinned external Python pattern and literal native fix. Native JSON and SARIF, scan/dryrun/apply commands and bytes are retained. Selected byte spans and protected behavior oracle gate application. Regex fixes, taint fixes, overlapping edits, other languages and experimental CLI are withheld.'}
        return {'complete':True,'count':len(value['results']),'issues':value['results'],'native':native}

    def semgrep_native(self, stage, output, label, fix):
        self.configured()
        rule = self.selection['rule_file']
        rule_copy = self.attempt / (label+'-rule.json')
        rule_bytes = Path(rule['path']).read_bytes()
        if sha(rule_bytes) != rule['sha256']:raise ValueError('Reviewed Semgrep rule changed during capture')
        rule_copy.write_bytes(rule_bytes)
        # Semgrep's parser recovery can report errors=[] and ~100% parsed for
        # a file Python rejects. Require the selected interpreter's compiler;
        # compile() produces no bytecode files and never executes target code.
        compiler_code = "import json,sys;from pathlib import Path\nrows=[]\nfor name in sys.argv[1:]:\n try: compile(Path(name).read_bytes(),name,'exec');rows.append({'path':name,'compiled':True})\n except (SyntaxError,ValueError,UnicodeError) as error: rows.append({'path':name,'compiled':False,'error':str(error)})\nprint(json.dumps({'python':sys.version,'files':rows}));sys.exit(0 if all(row['compiled'] for row in rows) else 1)"
        compiler_argv = [self.config['python']['path'], '-I', '-c', compiler_code, *self.selection['editable_files']]
        compiler_exit, compiler_stdout, _ = self.run(compiler_argv,stage,label+'-python-compile')
        compiler_report = json.loads(compiler_stdout)
        if compiler_exit or len(compiler_report.get('files',[])) != len(self.selection['editable_files']) or not all(row.get('compiled') is True for row in compiler_report['files']):
            raise ValueError('Python compiler did not complete the entire selected parse/rule/file scope')
        container = self.runtime.get('container') or {}
        docker = str(Path(self.runtime['executable']).resolve(strict=True))
        image = container.get('image', '')
        if not re.fullmatch('sha256:[0-9a-f]{64}', image) or self.runtime.get('prefix') != ['/opt/python/bin/semgrep'] or sha(Path(docker).read_bytes()) != self.runtime.get('sha256'):
            raise ValueError('Pinned native Semgrep container runtime is unavailable')
        code, out, _ = self.run([docker,'image','inspect',image], self.attempt, 'semgrep-image', sandbox=False)
        images = json.loads(out)
        if code or len(images)!=1 or images[0]['Id']!=image:raise ValueError('Semgrep container identity changed')
        captures=[]
        if any(',' in str(path) for path in [stage, rule_copy, *[stage/name for name in self.selection['editable_files']]]):
            raise ValueError('Comma-containing bind paths require another native adapter')
        phases = ['dryrun','apply'] if fix else ['scan']
        initial = self.snapshot(stage,label+'-before')
        for phase in phases:
            cidfile = self.attempt / (label+'-'+phase+'.cid')
            argv = [docker,'create','--cidfile',str(cidfile),'--label','org.thruline.native-repair-attempt='+sha(str(self.attempt).encode()),
                    '--network','none','--read-only','--cpus','1','--memory','2g','--pids-limit','128','--cap-drop','ALL',
                    '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}',
                    '--tmpfs','/tmp:rw,size=256m','--env','HOME=/tmp','--env','SEMGREP_ENABLE_VERSION_CHECK=0',
                    '--env','SEMGREP_SEND_METRICS=off','--env','DO_NOT_TRACK=1','--workdir','/input',
                    '--mount','type=bind,src='+str(stage)+',dst=/input,readonly',
                    '--mount','type=bind,src='+str(rule_copy)+',dst=/policy/rule.json,readonly']
            if phase == 'apply':
                for name in self.selection['editable_files']:
                    argv += ['--mount','type=bind,src='+str(stage/name)+',dst=/input/'+name]
            native = ['/opt/python/bin/semgrep','scan','--config','/policy/rule.json','--json','--metrics=off','--disable-version-check',
                      '--strict','--error','--disable-nosem','--no-git-ignore','--no-rewrite-rule-ids','--jobs','1','--timeout','20','--max-memory','2048']
            if phase != 'scan':native.append('--autofix')
            if phase == 'dryrun':native.append('--dryrun')
            native += ['/input/'+name for name in self.selection['editable_files']]
            guard='version=$(/opt/python/bin/semgrep --version) || exit 97; [ "$version" = "1.176.0" ] || exit 98; exec "$@"'
            argv += [image,'/bin/sh','-c',guard,'thruline-native-semgrep',*native]
            created, _, _ = self.run(argv,self.attempt,label+'-'+phase+'-create',sandbox=False)
            cid = cidfile.read_text().strip()
            if created or not re.fullmatch('[0-9a-f]{64}',cid):raise ValueError('Semgrep container identity not captured before workload')
            code, out, err = self.run([docker,'start','--attach',cid], self.attempt,label+'-'+phase,sandbox=False)
            inspect_code, state_out, _ = self.run([docker,'inspect',cid],self.attempt,label+'-'+phase+'-state',sandbox=False)
            state=json.loads(state_out)[0];host=state['HostConfig']
            expected_mounts = sorted([(str(stage), '/input', False), (str(rule_copy), '/policy/rule.json', False)] +
                ([(str(stage/name), '/input/'+name, True) for name in self.selection['editable_files']] if phase == 'apply' else []))
            actual_mounts = sorted((m['Source'],m['Destination'],m['RW']) for m in state['Mounts'])
            if inspect_code or state['Image']!=image or host['NetworkMode']!='none' or not host['ReadonlyRootfs'] or host['Memory']!=2*1024**3 or host['PidsLimit']!=128 or host['NanoCpus']!=1000000000 or host['CapDrop']!=['ALL'] or 'no-new-privileges' not in host['SecurityOpt'] or actual_mounts!=expected_mounts or state['State']['Running'] or state['State']['OOMKilled'] or state['State']['Pid']!=0:
                raise ValueError('Native Semgrep isolation or terminal state differs from contract')
            self.run([docker,'rm','-f',cid],self.attempt,label+'-'+phase+'-remove',sandbox=False)
            value=json.loads(out)
            complete=code in [0,1] and state['State']['ExitCode']==code and value.get('version')=='1.176.0' and value.get('engine_requested')=='OSS' and value.get('errors')==[] and value.get('skipped_rules')==[] and not value.get('paths',{}).get('skipped') and isinstance(value.get('results'),list) and sorted(value.get('paths',{}).get('scanned',[]))==sorted('/input/'+x for x in self.selection['editable_files'])
            record={'phase':phase,'complete':complete,'exit_code':code,'report':value,'native_argv':native,'container_id':cid,'container_state':state,'stderr':err.decode(errors='replace')}
            if phase != 'apply':
                after = self.snapshot(stage,label+'-'+phase+'-after')
                record.update(source_before_sha256=sha(encoded(initial)),source_after_sha256=sha(encoded(after)),readonly_source_unchanged=initial==after)
            captures.append(record);write(self.attempt/(label+'-'+phase+'-capture.json'),record)
            self.configured()
            if sha(rule_copy.read_bytes()) != rule['sha256']:raise ValueError('Captured Semgrep rule changed during native execution')
            if not complete:raise ValueError('Native Semgrep did not complete the entire selected parse/rule/file scope')
            if phase != 'apply' and not record['readonly_source_unchanged']:
                raise ValueError('Native read-only scan/dryrun changed source')
            for issue in value.get('results',[]):
                if issue.get('check_id')!=rule['rule_id'] or issue.get('path') not in ['/input/'+x for x in self.selection['editable_files']]:
                    raise ValueError('Native Semgrep reported a different rule or source')
        artifacts=[{'path':str(p),'sha256':sha(p.read_bytes()),'bytes':p.stat().st_size} for p in sorted(self.attempt.glob('*')) if p.is_file() and label in p.name and p.suffix in ['.stdout','.stderr','.json','.cid']]
        envelope={'schema':'thruline.semgrep-repair-capture.v1','version':'1.176.0','rule':rule,'executed_rule_sha256':sha(rule_copy.read_bytes()),'selected_files':self.selection['editable_files'],'captures':captures,'python_syntax':compiler_report,'artifacts':artifacts,'scope':'One pinned Python pattern+literal native rulefix; native syntax/parse success is not independent safety'}
        return {'complete':True,'count':len(captures[0]['report']['results']),'issues':captures[0]['report']['results'],'native':envelope}

    def cargo_fix(self, stage, output, label):
        # Cargo's native fixer uses TCP for two local IPC servers. Host-wide
        # loopback exceptions are not permitted; a reviewed closed-network
        # container is required and must be independently revalidated.
        image = self.selection.get('fix_container')
        if not image: raise ValueError('Native Cargo fix needs a pinned network-none Rust/Clippy container for its local locking IPC; no host network exception is granted')
        if set(image) != {'image_id','clippy_version','docker_sha256'} or not re.fullmatch('sha256:[0-9a-f]{64}',image['image_id']) or image['clippy_version']!='0.1.97':
            raise ValueError('Invalid pinned native Cargo-fix container contract')
        docker='/usr/local/bin/docker'
        if sha(Path(docker).read_bytes())!=image['docker_sha256']:raise ValueError('Reviewed Docker client changed')
        code,out,_=self.run([docker,'image','inspect',image['image_id']],self.attempt,'inspect-fix-image',sandbox=False)
        inspected=json.loads(out)
        if code or len(inspected)!=1 or inspected[0]['Id']!=image['image_id']:raise ValueError('Native fix image is missing or replaced')
        cfg=read(stage/'.thruline/clippy.json')
        v2=cfg.get('schema')=='thruline.clippy.v2'
        lint='clippy::unnecessary_map_or' if v2 else 'clippy::needless_return'
        if cfg.get('schema') not in ('thruline.clippy.v1','thruline.clippy.v2') or cfg.get('lints')!=[lint] or cfg.get('targets') not in ['lib','all-targets']:
            raise ValueError('Only the captured named Clippy compilation scope is repairable')
        if v2 and cfg.get('suppression')!='force-warn': raise ValueError('native repair requires explicit force-warn')
        if v2:
            relative=Path(cfg['clippy_config'])
            if relative.is_absolute() or '..' in relative.parts or relative.name not in ('clippy.toml','.clippy.toml'):
                raise ValueError('invalid selected native Clippy policy')
            if not (stage/relative).is_file() or (stage/relative).is_symlink(): raise ValueError('selected native Clippy policy missing or symlinked')
        features=cfg['features']
        if not isinstance(features,list) or any(not isinstance(f,str) or not re.fullmatch('[A-Za-z0-9_/-]+',f) for f in features):raise ValueError('Invalid Clippy feature selection')
        cidfile=self.attempt/'cargo-fix.cid'
        controls=['--read-only','--network','none','--cpus','2','--memory','2g','--pids-limit','128','--cap-drop','ALL',
                  '--security-opt','no-new-privileges','--user',f'{os.getuid()}:{os.getgid()}',
                  '--tmpfs','/tmp:rw,size=256m','--env','CARGO_HOME=/tmp/cargo','--env','CARGO_NET_OFFLINE=true']
        if v2: controls += ['--env','CLIPPY_CONF_DIR=/input/'+str(Path(cfg['clippy_config']).parent)]
        args=['cargo','clippy','--fix','--allow-dirty','--allow-staged','--offline','--locked','--workspace',
              '--'+cfg['targets'],'--target-dir','/output/target','--message-format=json']
        if not cfg['default_features']:args.append('--no-default-features')
        if features:args.extend(['--features',','.join(features)])
        args.extend(['--','-A','warnings','--force-warn',lint])
        argv=[docker,'create','--cidfile',str(cidfile),'--label','org.thruline.native-repair-attempt='+sha(str(self.attempt).encode()),
              *controls,'--workdir','/input','--mount','type=bind,src='+str(stage)+',dst=/input,readonly',
              '--mount','type=bind,src='+str(output)+',dst=/output']
        for name in self.selection['editable_files']:
            argv.extend(['--mount','type=bind,src='+str(stage/name)+',dst=/input/'+name])
        # Version and work share the tracked container; cancellation cannot leave
        # an untracked version-probe workload behind. This shell text is fixed,
        # while Cargo arguments remain positional data.
        version_guard = 'version=$(cargo clippy --version) || exit 97; printf "%s\\n" "$version" >&2; case "$version" in "clippy 0.1.97 "*) ;; *) exit 98;; esac; exec "$@"'
        argv.extend([image['image_id'],'/bin/sh','-c',version_guard,'thruline-native-fix',*args])
        created,_,_=self.run(argv,self.attempt,'create-fixer-container',sandbox=False)
        cid=cidfile.read_text().strip()
        if created or not re.fullmatch('[0-9a-f]{64}',cid):raise ValueError('Container identity was not captured before execution')
        code,out,err=self.run([docker,'start','--attach',cid],self.attempt,label,sandbox=False)
        state_code,state_out,_=self.run([docker,'inspect',cid],self.attempt,'fixer-container-state',sandbox=False)
        state=json.loads(state_out)[0]
        hosts=state['HostConfig']
        if state_code or hosts['NetworkMode']!='none' or not hosts['ReadonlyRootfs'] or hosts['Memory']!=2*1024**3 or hosts['PidsLimit']!=128:
            raise ValueError('Native fixer container isolation differs from its fixed contract')
        self.run([docker,'rm','-f',cid],self.attempt,'remove-fixer-container',sandbox=False)
        rows=[]
        for line in out.splitlines():rows.append(json.loads(line))
        if code or state['State']['Running'] or state['State']['ExitCode']!=0:raise ValueError('Native container Cargo-fix failed; original source remains unchanged')
        return {'complete':True,'native_events':rows,'native_stderr':err.decode(errors='replace'),
                'container_image':image['image_id'],'container_id':cid,'container_state':state,
                'scope':'Native Linux cargo fix inside network-none namespace; exact host MachineApplicable edits and macOS detector/oracle are checked separately.'}

    def expected(self, stage, original):
        expected = {}
        if self.pack == 'osv-scanner':
            return {'package-lock.json':original['proposed_lock'].encode()}
        if self.pack in ['semgrep', 'opengrep']:
            edits = {}
            for issue in original['issues']:
                name = issue['path'].removeprefix('/input/')
                if name not in self.selection['editable_files'] or issue['check_id'] != self.selection['rule_file']['rule_id']:
                    raise ValueError('Native fix escaped selected file/rule identity')
                replacement = issue.get('extra', {}).get('fix')
                if not isinstance(replacement, str): raise ValueError('Native finding has no literal supported fix')
                start, end = issue.get('start', {}).get('offset'), issue.get('end', {}).get('offset')
                data = (stage/name).read_bytes()
                if type(start) is not int or type(end) is not int or not 0 <= start < end <= len(data):
                    raise ValueError('Native fix has invalid source byte offsets')
                edits.setdefault(name, []).append((start,end,replacement.encode()))
            for name, spans in edits.items():
                data = (stage/name).read_bytes();spans=sorted(set(spans))
                if any(a[1] > b[0] for a,b in zip(spans,spans[1:])):raise ValueError('Overlapping native '+self.pack.title()+' fixes are withheld')
                for start,end,replacement in reversed(spans):data=data[:start]+replacement+data[end:]
                expected[name]=data
        elif self.pack == 'openrewrite':
            for issue in original['issues']:
                name = issue['path']
                if name not in self.selection['editable_files'] or (stage/name).read_text()!=issue['before']:
                    raise ValueError('Recipe scope or before source differs')
                expected[name] = issue['after'].encode()
        elif self.pack == 'knip':
            for issue in original['issues']:
                name = issue['file']
                if name not in self.selection['editable_files']: raise ValueError('Knip has unused exports outside selected repair scope')
                text = expected.get(name, (stage/name).read_bytes()).decode()
                symbol = issue.get('name', issue.get('symbol'))
                if not isinstance(symbol,str): raise ValueError('Unknown native unused export identity')
                pattern = r'(?m)^([ \t]*)export[ \t]+(?=(?:async[ \t]+)?(?:function|const|let|class)[ \t]+'+re.escape(symbol)+r'\b)'
                text, count = re.subn(pattern, r'\1', text)
                if count != 1: raise ValueError('Only simple named unused export modifiers are supported; complex exports remain withheld')
                expected[name] = text.encode()
        else:
            edits = {}
            for issue in original['issues']:
                pending = [issue]
                while pending:
                    diagnostic = pending.pop(); pending.extend(diagnostic.get('children',[]))
                    for span in diagnostic.get('spans',[]):
                        if span.get('suggested_replacement') is None: continue
                        if span.get('suggestion_applicability') != 'MachineApplicable': raise ValueError('Non-machine-applicable Clippy suggestion is withheld')
                        name = span['file_name']
                        if name not in self.selection['editable_files']: raise ValueError('Clippy suggested an edit outside selected files')
                        edits.setdefault(name,[]).append((span['byte_start'],span['byte_end'],span['suggested_replacement'].encode()))
            for name, spans in edits.items():
                data = (stage/name).read_bytes(); spans = sorted(set(spans))
                if any(a<0 or b<a or b>len(data) for a,b,_ in spans) or any(left[1]>right[0] for left,right in zip(spans,spans[1:])):
                    raise ValueError('Conflicting native suggestion spans')
                for start,end,after in reversed(spans): data=data[:start]+after+data[end:]
                expected[name]=data
        if not expected: raise ValueError('No supported native edit was captured')
        return expected

    def preview(self):
        if Path(self.git(['rev-parse','--show-toplevel'], label='root').decode().strip()).resolve()!=self.root:
            raise ValueError('Select the exact Git repository root for native repairs')
        snapshot=self.snapshot(self.root,'source-before'); head=self.git(['rev-parse','HEAD'],label='base-commit').decode().strip()
        self.digest=sha(encoded({'snapshot':snapshot,'head':head,'config':self.config,'candidate':self.request['candidate_sha256'],'nonce':uuid.uuid4().hex}))
        directory=self.store/'repairs'/self.digest; directory.mkdir(parents=True)
        base,stage=directory/'base',directory/'stage'; base.mkdir();stage.mkdir()
        for name,identity in snapshot['files'].items():
            if identity is not None:
                for destination in [base/name,stage/name]:
                    destination.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(self.root/name,destination)
        for destination in [base,stage]:
            self.git(['init','--quiet'],destination,'initialize-stage-index');self.git(['add','--all'],destination,'index-stage-source')
        original=self.native(stage,'original-detector')
        if not original['count']: raise ValueError('No current selected native issue requires a repair')
        expected=self.expected(stage,original)
        fixed=self.native(stage,'native-fix',fix=True)
        staged=self.snapshot(stage,'native-fix-scope')
        if set(staged['files']) != set(snapshot['files']): raise ValueError('Native fix added or removed source files')
        changed=[];patch=[]
        for name,identity in snapshot['files'].items():
            if identity is None: continue
            if staged['files'][name] is None or staged['files'][name]['mode'] != identity['mode']:
                raise ValueError('Native fix changed file presence or permissions')
            before=(base/name).read_bytes();after=(stage/name).read_bytes()
            if before!=after:
                if expected.get(name)!=after: raise ValueError('Native fix differs from captured supported edits or changed unselected source')
                changed.append(name);patch.extend(difflib.unified_diff(before.decode().splitlines(keepends=True),after.decode().splitlines(keepends=True),'a/'+name,'b/'+name))
        if not changed: raise ValueError('Native fixer produced no patch')
        patch=''.join(patch);(directory/'change.patch').write_text(patch)
        if self.snapshot(self.root,'source-after-preview')!=snapshot: raise ValueError('Original source changed during preview')
        value={'schema':'thruline.native-repair.v1','recipe':self.pack,'state':'previewed','digest':self.digest,'root':str(self.root),
               'base_commit':head,'base_snapshot':snapshot,'stage_snapshot':staged,'config_sha256':sha(encoded(self.config)),
               'candidate_sha256':self.request['candidate_sha256'],'changed_files':changed,'diff':patch,'patch_sha256':sha(patch.encode()),
               'original_detector':original,'native_fix':fixed,'artifacts':str(directory),'history':[{'action':'preview','attempt':str(self.attempt),'state':'previewed'}],
               'limitations':['Only selected captured native edits are supported; other fix categories are withheld.',snapshot['scope'],
               'Named independent checks bound behavior; this is not whole-product correctness or independent tamper-resistant custody.']}
        write(directory/'repair.json',value);return {**value,'meta_sha256':sha(encoded(value))}

    def current(self,directory,meta):
        if meta['root']!=str(self.root) or meta['recipe']!=self.pack or meta['config_sha256']!=sha(encoded(self.config)) or meta['candidate_sha256']!=self.request['candidate_sha256']:
            raise ValueError('Source root, runtime, selection, oracle or candidate changed; preview again')
        if self.git(['rev-parse','HEAD'],label='current-commit').decode().strip()!=meta['base_commit']:
            raise ValueError('Git base changed after preview')
        if self.snapshot(self.root,'current-source')!=meta['base_snapshot']:raise ValueError('Source changed after preview; patch is stale')
        if self.snapshot(directory/'base','current-base')!=meta['base_snapshot']:raise ValueError('Retained baseline changed')
        if self.snapshot(directory/'stage','current-stage')!=meta['stage_snapshot']:raise ValueError('Staged patch changed')
        if sha((directory/'change.patch').read_bytes())!=meta['patch_sha256']:raise ValueError('Patch artifact changed')

    def proceed(self,action):
        if not re.fullmatch('[0-9a-f]{64}',self.digest):raise ValueError('A current repair digest is required')
        directory=self.store/'repairs'/self.digest;meta=read(directory/'repair.json')
        if sha(encoded(meta))!=self.request.get('prior_meta_sha256'):raise ValueError('Repair metadata differs from its last captured receipt')
        try:
            self.current(directory,meta)
            if action=='verify':
                baseline=self.oracle(directory/'base','baseline');changed=self.oracle(directory/'stage','changed')
                detector=self.native(directory/'stage','detector-recheck');self.configured();self.current(directory,meta)
                meta['verification']={'base':baseline,'changed':changed,'detector_cleared':detector['complete'] and detector['count']==0,'remaining_detector':detector}
                meta['state']='verified_for_named_checks' if baseline['passed'] and changed['passed'] and detector['count']==0 else 'verification_failed'
            elif action=='apply':
                if meta['state']!='verified_for_named_checks':raise ValueError('Independent verification must pass before apply')
                self.git(['apply','--check',str(directory/'change.patch')],label='apply-preflight');self.configured();self.current(directory,meta)
                self.git(['apply',str(directory/'change.patch')],label='apply-verified-patch')
                after=self.snapshot(self.root,'applied-source')
                if after!=meta['stage_snapshot']:raise ValueError('Applied source differs from verified stage; inspect retained originals')
                meta['state']='applied_verified_patch';meta['applied_snapshot']=after
            else:raise ValueError('Unsupported repair action')
        except Exception as error:meta['state']='rejected';meta['error']=str(error)
        meta['history'].append({'action':action,'attempt':str(self.attempt),'state':meta['state'],'error':meta.get('error')})
        write(directory/'repair.json',meta);return {**meta,'meta_sha256':sha(encoded(meta))}

    def execute(self):
        self.configured()
        return self.preview() if self.request['action']=='preview' else self.proceed(self.request['action'])


def recover_attempt(request_path, expected_request_sha256):
    """Read-only recovery. Caller must first account for native process cleanup.

    This entry point cannot invoke Broker.execute, native, oracle, Git or Docker.
    It opens only the exact request/control and declared direct attempt children.
    The existing descriptor-relative collector handles file bytes and budgets.
    """
    attempt = request_path.parent
    empty = {'schema':'thruline.native-repair-attempt-artifacts.v1','complete':False,
             'roots':[],'artifacts':[],'gaps':[]}
    response = {'schema':'thruline.native-repair-artifact-recovery.v1',
                'capture_only':True,'attempt':str(attempt),'request_sha256':expected_request_sha256,
                'broker_sha256':sha(Path(__file__).read_bytes()),'attempt_artifacts':empty}
    base = None
    try:
        if request_path.name != 'request.json' or not attempt.is_absolute() or attempt.resolve(strict=True) != attempt:
            raise ValueError('recovery requires the exact ordinary attempt request')
        if not re.fullmatch('[0-9a-f]{64}',expected_request_sha256):
            raise ValueError('expected request digest missing')
        base = os.open(attempt,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        def bounded(name, limit):
            fd = os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=base)
            try:
                before=os.fstat(fd)
                if not stat.S_ISREG(before.st_mode) or before.st_size>limit:raise ValueError('unsafe or oversized recovery control')
                data=b''
                while len(data)<=limit:
                    chunk=os.read(fd,min(65536,limit+1-len(data)))
                    if not chunk:break
                    data+=chunk
                after=os.fstat(fd)
                if len(data)!=before.st_size or (before.st_size,before.st_mtime_ns,before.st_ctime_ns)!=(after.st_size,after.st_mtime_ns,after.st_ctime_ns):raise ValueError('recovery control changed during read')
                return data
            finally:os.close(fd)
        request_bytes=bounded('request.json',1024*1024)
        if sha(request_bytes)!=expected_request_sha256:raise ValueError('repair request changed before capture recovery')
        request=json.loads(request_bytes);control_bytes=bounded('capture-control.json',65536);control=json.loads(control_bytes)
        expected={'schema','attempt','root','store','request_sha256','broker_sha256','roots','generation','capture_only_protocol'}
        if set(control)!=expected or control['schema']!='thruline.native-repair-capture-control.v1' or control['capture_only_protocol'] is not True:
            raise ValueError('invalid durable capture-control schema')
        if any(control[k]!=request[k] for k in ['attempt','root','store']) or control['attempt']!=str(attempt) or control['request_sha256']!=expected_request_sha256 or control['broker_sha256']!=response['broker_sha256']:
            raise ValueError('durable capture-control request, root or broker identity differs')
        roots=control['roots']
        if not isinstance(roots,list) or len(roots)>128 or control['generation']!=len(roots):raise ValueError('invalid durable capture root count')
        names=set();selected=[]
        for row in roots:
            if not isinstance(row,dict) or set(row)!={'relative_path','kind'} or row['kind'] not in ['native','oracle'] or not isinstance(row['relative_path'],str) or not re.fullmatch('[a-z0-9][a-z0-9-]{0,127}',row['relative_path']) or row['relative_path'] in names:
                raise ValueError('unsafe or duplicate durable capture root')
            names.add(row['relative_path']);selected.append((attempt/row['relative_path'],row['kind']))
        response['control']={'sha256':sha(control_bytes),'bytes':len(control_bytes),'value':control}
        collector=object.__new__(Broker);collector.attempt=attempt;collector.capture_roots=selected
        response['attempt_artifacts']=collector.attempt_artifacts()
    except (OSError,ValueError,KeyError,TypeError) as error:
        empty['gaps'].append({'path':str(attempt),'reason':str(error)})
    finally:
        if base is not None:os.close(base)
    return response


def main():
    if len(sys.argv)==4 and sys.argv[1]=='--capture-attempt':
        print(json.dumps(recover_attempt(Path(sys.argv[2]),sys.argv[3])));return
    request_path=Path(sys.argv[1]);request=read(request_path);broker=Broker(request,sha(request_path.read_bytes()))
    with (broker.store/'repair.lock').open('a') as lock:
        try:
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);value=broker.execute()
        except Exception as error:
            value={'schema':'thruline.native-repair.v1','recipe':broker.pack,'state':'unavailable' if isinstance(error,FileNotFoundError) else 'failed',
                   'error':str(error),'attempt':str(broker.attempt),'digest':broker.digest,'root':str(broker.root)}
        if value['state'] not in ['previewed','verified_for_named_checks','applied_verified_patch']:
            try: value['attempt_artifacts'] = broker.attempt_artifacts()
            except Exception as error:
                value['attempt_artifacts'] = {'schema':'thruline.native-repair-attempt-artifacts.v1','complete':False,
                    'roots':[], 'artifacts':[], 'gaps':[{'path':str(broker.attempt),'reason':str(error)}]}
        write(broker.attempt/'result.json',value);print(json.dumps(value))


if __name__=='__main__':main()
