"""Owned static Python import collector. Never imports examined project code.
Bounded graph only: explicit modules, package initialization and syntactic imports.
Conditional imports are edges too; runtime reachability/architecture quality is not inferred.
"""
import ast
import json
import itertools
import pathlib
import sys

MAX_MODULES, MAX_EDGES = 512, 8192

def collect_python(plan):
    root = pathlib.Path(plan['root'])
    policy = plan.get('policy')
    value = dict(protocol='thruline-architecture/3', plan=plan, modules=[], edges=[], cycles=[], issues=[],
                 method='Python AST module dependencies; conditional imports included; exported symbol identities not inferred; project code not executed')
    issues, edges = value['issues'], value['edges']
    if not root.is_dir():
        issues.append(dict(path=str(root), line=0, reason='Declared Python source root is not a directory.'))
        return value
    modules, texts, parsed = {}, {}, {}
    for file in sorted(root.rglob('*')):
        if '__pycache__' in file.parts or '.git' in file.parts or not file.is_file(): continue
        if file.suffix in {'.js','.jsx','.ts','.tsx','.rs','.go','.so','.pyd','.pyc'}:
            issues.append(dict(path=str(file), line=0, reason='Unsupported language or native/generated import input inside the declared Python root.'))
            continue
        if file.suffix != '.py': continue
        rel = file.relative_to(root).with_suffix('')
        parts = list(rel.parts)
        if parts[-1] == '__init__': parts.pop()
        if not parts or not all(p.isidentifier() for p in parts):
            issues.append(dict(path=str(file), line=0, reason='Root package or nonstandard module layout is unsupported; select its parent source root.'))
            continue
        name = '.'.join(parts)
        if name in modules:
            issues.append(dict(path=str(file), line=0, reason='Ambiguous module/package identity: '+name)); continue
        if len(modules) >= MAX_MODULES: raise ValueError('Source root exceeds 512 Python modules')
        modules[name] = file
        text = file.read_text(encoding='utf-8')
        texts[name] = text
        try: parsed[name] = ast.parse(text, filename=str(file))
        except (SyntaxError, ValueError) as e:
            issues.append(dict(path=str(file), line=getattr(e,'lineno',0), reason='Python source cannot be parsed: '+str(e)[:300]))
    value['modules'] = [dict(name=n,path=str(f)) for n,f in modules.items()]
    if not modules: issues.append(dict(path=str(root),line=0,reason='No supported Python modules found.'))
    external = (policy or {}).get('external_modules', [])
    def known_external(name): return any(name == p or name.startswith(p+'.') for p in external)
    def add(src, target, node, implicit=False):
        if len(edges) >= MAX_EDGES: raise ValueError('Import graph exceeds 8192 edges')
        if target not in modules and not known_external(target):
            issues.append(dict(path=str(modules[src]),line=node.lineno,reason='Unresolved import '+target+'; add its source or explicitly declare its external boundary.'))
            return
        edge = dict(source=src,target=target,path=str(modules[src]),line=node.lineno,
                    text=(ast.get_source_segment(texts[src],node) or '')[:400],implicit_package=implicit)
        if edge not in edges: edges.append(edge)
        if not implicit:
            parts=target.split('.')
            for i in range(1,len(parts)):
                ancestor='.'.join(parts[:i])
                if ancestor in modules and modules[ancestor].name == '__init__.py' and ancestor != src:
                    add(src,ancestor,node,True)
    for src, tree in parsed.items():
        for node in ast.walk(tree):
            if isinstance(node, (ast.Import, ast.ImportFrom)) and (isinstance(node, ast.ImportFrom) and node.module in {'importlib','builtins','pkgutil'} or isinstance(node, ast.Import) and any(a.name in {'importlib','builtins','pkgutil'} for a in node.names)):
                issues.append(dict(path=str(modules[src]),line=node.lineno,reason='An import-resolution API is imported; aliasing and dynamic module loading are outside static closure.'))
        package = src if modules[src].name == '__init__.py' else src.rpartition('.')[0]
        for node in ast.walk(tree):
            if isinstance(node,ast.Import):
                for alias in node.names: add(src,alias.name,node)
            elif isinstance(node,ast.ImportFrom):
                base=node.module or ''
                if node.level:
                    parts=package.split('.') if package else []
                    if node.level > len(parts):
                        issues.append(dict(path=str(modules[src]),line=node.lineno,reason='Relative import escapes the declared source root.')); continue
                    base='.'.join(parts[:len(parts)-node.level+1]+([base] if base else []))
                # A wildcard still initializes its named module. ARC-01/02
                # inspect module dependencies, not the imported symbol set.
                # All local module bodies are inspected above; declared external
                # modules are accepted leaf boundaries just as for plain imports.
                # A source package is different: Python may load submodules named
                # by __all__, including lazy/re-exported names not expressed as
                # imports. Do not claim that package export closure is complete.
                if any(a.name == '*' for a in node.names) and base in modules and modules[base].name == '__init__.py':
                    issues.append(dict(path=str(modules[src]),line=node.lineno,reason='Wildcard import from a source package may initialize re-exported submodules; package export closure is incomplete.'))
                # Importing a member always initializes its package/module as well.
                if base in modules or known_external(base): add(src,base,node)
                elif any(a.name == '*' for a in node.names): add(src,base,node)
                for alias in node.names:
                    if alias.name == '*': continue
                    target=base+'.'+alias.name if base else alias.name
                    if target in modules: add(src,target,node)
                    elif base not in modules and not known_external(base): add(src,target,node)
            elif (isinstance(node,ast.Call) and ((isinstance(node.func,ast.Name) and node.func.id in {'__import__','exec','eval'}) or (isinstance(node.func,ast.Attribute) and node.func.attr in {'import_module','exec_module','load_module','spec_from_file_location'}))) or (isinstance(node,(ast.FunctionDef,ast.AsyncFunctionDef)) and node.name == '__getattr__') or (isinstance(node,ast.Attribute) and isinstance(node.value,ast.Name) and node.value.id == 'sys' and node.attr in {'path','meta_path','modules','path_hooks'}):
                issues.append(dict(path=str(modules[src]),line=node.lineno,reason='Dynamic import or module-resolution behavior needs runtime evidence; static closure is incomplete.'))
    return finish_graph(value)


def finish_graph(value):
    """Bounded strongly-connected components and a directed source-edge witness."""
    edges = value['edges']
    modules = {m['name'] for m in value['modules']}
    graph={n:sorted({e['target'] for e in edges if e['source']==n and e['target'] in modules}) for n in modules}
    reach={}
    for n in graph:
        seen=set(); todo=list(graph[n])
        while todo:
            k=todo.pop()
            if k in seen: continue
            seen.add(k); todo.extend(graph[k])
        reach[n]=seen
    remaining=set(graph)
    while remaining:
        n=min(remaining); comp=sorted(k for k in remaining if k==n or k in reach[n] and n in reach[k]); remaining.difference_update(comp)
        if len(comp)==1 and n not in graph[n]: continue
        # Find a simple cycle within this SCC, retaining the exact source edges.
        def witness(start):
            queue=[(start,[start])]
            for current,path in queue:
                for target in graph[current]:
                    if target not in comp: continue
                    if target==start: return path+[start]
                    if target not in path and not any(target==x[0] for x in queue): queue.append((target,path+[target]))
        path=witness(n)
        value['cycles'].append(dict(modules=comp,path=path,edges=[next(e for e in edges if e['source']==a and e['target']==b) for a,b in zip(path,path[1:])]))
    return value


def collect_rust(plan):
    """Declared Cargo package edges, not Rust use/mod imports or resolved versions.

    Includes normal/build/dev, optional and all target-conditional declarations.
    Workspace inheritance, dependency renames, members/excludes and local path
    dependencies are resolved without invoking Cargo or executing project code.
    Registry/git packages are explicit leaf boundaries; their internals are absent.
    """
    import fnmatch
    import tomllib
    root = pathlib.Path(plan['root'])
    value = dict(protocol='thruline-architecture/4', plan=plan, graph_type='rust_packages',
                 modules=[], edges=[], cycles=[], issues=[], input_manifests=[],
                 method='Cargo manifest package dependencies; all normal/build/dev/optional/target declarations; no project execution',
                 limits=['Not a Rust module-import graph or resolved Cargo.lock version graph.',
                         'Conditional declarations are included regardless of activation; a cycle is not necessarily build-reachable.',
                         'Registry/git package internals and transitive external relationships are outside this graph.'])
    issues, edges = value['issues'], value['edges']
    def issue(path, reason, line=0):
        issues.append(dict(path=str(path), line=line, reason=reason))
    def read(path):
        if not path.resolve().is_relative_to(root.resolve()):
            raise ValueError('Cargo manifest escapes declared root: '+str(path))
        if path.is_file():
            relative = path.resolve().relative_to(pathlib.Path.cwd().resolve()).as_posix()
            if relative not in value['input_manifests']:
                if len(value['input_manifests']) >= MAX_MODULES + 1:
                    raise ValueError('Cargo manifest inventory exceeds 513 files')
                value['input_manifests'].append(relative)
        if path.stat().st_size > 1024 * 1024:
            raise ValueError('Cargo manifest exceeds 1 MiB: '+str(path))
        return tomllib.loads(path.read_text(encoding='utf-8'))
    def leaf_path(obj):
        parts=[]
        while isinstance(obj,dict) and len(obj)==1:
            key,obj=next(iter(obj.items()));parts.append(key)
        return tuple(parts)
    def locations(path):
        table=(); found={}; quote=None; depth=0
        for line_no,raw in enumerate(path.read_text(encoding='utf-8').splitlines(),1):
            line=raw.strip()
            # A header-looking line inside a multiline value is not a table.
            # The complete document was already parsed by tomllib; this bounded
            # lexer only locates statement starts, never interprets dependencies.
            statement=quote is None and depth == 0
            if statement and line.startswith('[') and not line.startswith('[['):
                try: table=leaf_path(tomllib.loads(line+'\n__location_marker__=1'))[:-1]
                except tomllib.TOMLDecodeError: table=()
                found[table]=line_no
            elif statement and '=' in line and not line.startswith('#'):
                try: key=leaf_path(tomllib.loads(line.split('=',1)[0]+'=1'))
                except tomllib.TOMLDecodeError: key=()
                # serde.workspace=true and serde={workspace=true} declare the
                # same dependency. Bind the first actual declaration of each
                # dotted prefix, including workspace-inherited source locations.
                for length in range(1,len(key)+1):
                    found.setdefault(table+key[:length],line_no)
            index=0
            while index < len(raw):
                char=raw[index]
                if quote is not None:
                    if quote[0]=='"' and char=='\\':
                        index+=2;continue
                    if raw.startswith(quote,index):
                        count=len(quote)
                        if count==3:
                            # Up to two trailing quotes belong to a valid TOML
                            # multiline string. Consume the whole closing run.
                            while index+count<len(raw) and raw[index+count]==quote[0]: count+=1
                        index+=count;quote=None;continue
                elif char=='#':
                    break
                elif char in ['"',"'"]:
                    quote=char*3 if raw.startswith(char*3,index) else char
                    index+=len(quote);continue
                elif char in '[{': depth+=1
                elif char in ']}': depth-=1
                index+=1
        return found
    try:
        top=root/'Cargo.toml'; top_doc=read(top)
    except (OSError, ValueError) as e:
        issue(root,str(e));return value
    workspace=top_doc.get('workspace',{})
    inherited=workspace.get('dependencies',{})
    pending=[top] if 'package' in top_doc else []
    excluded=workspace.get('exclude',[])
    for pattern in workspace.get('members',[]):
        matches=sorted(itertools.islice(root.glob(pattern), MAX_MODULES + 1))
        if len(matches) > MAX_MODULES:
            raise ValueError('Cargo workspace member pattern exceeds 512 matches')
        if not matches: issue(top,'Workspace member pattern has no match: '+pattern)
        for directory in matches:
            rel=directory.relative_to(root).as_posix()
            if not any(fnmatch.fnmatchcase(rel,p) for p in excluded): pending.append(directory/'Cargo.toml')
    loaded={}; names={}; top_locations=locations(top)
    while pending:
        manifest=pending.pop(0)
        resolved=manifest.resolve()
        if resolved in loaded: continue
        if len(loaded)>=MAX_MODULES: raise ValueError('Cargo graph exceeds 512 local packages')
        try: doc=read(manifest)
        except (OSError, ValueError) as e:
            issue(manifest,str(e));continue
        package=doc.get('package',{}).get('name')
        if not isinstance(package,str) or not package:
            issue(manifest,'No concrete Cargo package name.');continue
        name='package.'+package
        if name in names and names[name]!=resolved:
            issue(manifest,'Duplicate package identity: '+package);continue
        names[name]=resolved; loaded[resolved]=(name,manifest,doc)
        value['modules'].append(dict(name=name,path=str(manifest)))
        if 'patch' in doc or 'replace' in doc or (manifest != top and 'workspace' in doc):
            issue(manifest,'Cargo patches, replacements or nested workspaces need a resolved graph; this declared graph is incomplete.')
        at=locations(manifest)
        tables=[((kind,),doc[kind]) for kind in ['dependencies','dev-dependencies','build-dependencies'] if kind in doc]
        for condition,target in doc.get('target',{}).items():
            tables += [(('target',condition,kind),target[kind]) for kind in ['dependencies','dev-dependencies','build-dependencies'] if kind in target]
        for table,dependencies in tables:
            for alias,spec in sorted(dependencies.items()):
                line=at.get(table+(alias,),0)
                if not line: issue(manifest,'Dependency location could not be established: '+alias)
                spec={'version':spec} if isinstance(spec,str) else dict(spec)
                base=manifest.parent; inherited_location=None
                if spec.get('workspace'):
                    if alias not in inherited:
                        issue(manifest,'Workspace dependency is missing: '+alias,line);continue
                    extra=dict(spec);spec=inherited[alias]
                    spec={'version':spec} if isinstance(spec,str) else dict(spec)
                    # Package/path come from the workspace, while optional/features
                    # may be specified at the use site. Both declarations are captured.
                    spec.update({k:v for k,v in extra.items() if k in {'optional','features'}})
                    base=root
                    inherited_location=dict(path=str(top),line=top_locations.get(('workspace','dependencies',alias),0))
                actual=spec.get('package',alias)
                target='external.'+actual
                if 'path' in spec:
                    destination=base/spec['path']/'Cargo.toml'
                    try: target_doc=read(destination)
                    except (OSError, ValueError) as e:
                        issue(manifest,str(e),line);continue
                    actual_name=target_doc.get('package',{}).get('name')
                    if actual_name != actual:
                        issue(manifest,'Local dependency package name does not match '+actual,line);continue
                    target='package.'+actual;pending.append(destination)
                else:
                    external=(plan.get('policy') or {}).get('external_modules',[])
                    if not any(target==p or target.startswith(p+'.') for p in external):
                        issue(manifest,'Undeclared external boundary: '+target,line)
                if len(edges)>=MAX_EDGES: raise ValueError('Cargo graph exceeds 8192 dependency edges')
                edges.append(dict(source=name,target=target,path=str(manifest),line=line,
                                  text=alias,dependency_kind=table[-1],condition=table[1] if len(table)>1 else None,
                                  optional=bool(spec.get('optional')),alias=alias,workspace_declaration=inherited_location))
    if 'patch' in top_doc or 'replace' in top_doc:
        issue(top,'Workspace patches/replacements require resolved dependencies; declarations alone are incomplete.')
    if not value['modules']: issue(top,'No Cargo packages found in the declared root.')
    return finish_graph(value)


def collect(plan):
    graph=plan.get('graph','python_imports')
    if graph=='rust_packages': return collect_rust(plan)
    if graph!='python_imports': raise ValueError('Unsupported architecture graph: '+str(graph))
    value=collect_python(plan)
    value['graph_type']='python_imports'
    if 'graph' in plan: value['protocol']='thruline-architecture/4'
    return value

if __name__ == '__main__':
    plan=json.loads(sys.argv[1])
    try: value=collect(plan)
    except Exception as e: value=dict(protocol='thruline-architecture/4' if 'graph' in plan else 'thruline-architecture/3',plan=plan,graph_type=plan.get('graph','python_imports'),modules=[],edges=[],cycles=[],issues=[dict(path=plan['root'],line=0,reason='Collector incomplete: '+str(e)[:500])])
    if sys.argv[2:] == ['--manifest-inventory']:
        value = dict(protocol='thruline-cargo-inputs/1', files=value.get('input_manifests'))
    print(json.dumps(value,sort_keys=True))
