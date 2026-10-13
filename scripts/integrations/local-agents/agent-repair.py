#!/usr/bin/env python3
"""Embedded trusted proposal broker; native agents and oracles are sandboxed.

No model prose or native Submitted status establishes behavioral acceptance.
Only the configured protected oracle may authorize a named-check patch apply.
"""
import base64
import difflib
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time
import uuid

# Runtime helpers are immutable evidence inputs, including across child runs.
sys.dont_write_bytecode = True


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def write(path, value):
    temporary = path.with_name(path.name + ".new")
    temporary.write_bytes(encoded(value))
    temporary.replace(path)


def safe(path, root=None, limit=16 * 1024 * 1024):
    path = Path(path).absolute()
    if root is not None and not path.is_relative_to(root):
        raise ValueError("path outside authorized artifact/source root")
    if any(p.is_symlink() for p in [path, *path.parents]):
        raise ValueError("symlink refused: " + str(path))
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        metadata = os.fstat(stream.fileno())
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size > limit:
            raise ValueError("not a bounded regular file: " + str(path))
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError("artifact exceeded byte bound")
    return data


def read(path):
    return json.loads(safe(path))


def ref(path):
    data = safe(path)
    return {"path": str(path), "sha256": sha(data), "size": len(data)}


def check(binding):
    data = safe(Path(binding["path"]))
    if sha(data) != binding["sha256"] or binding.get("size", len(data)) != len(data):
        raise ValueError("bound file changed: " + binding["path"])
    return data


def load_bound_driver(config, name):
    matches = [item for item in config["drivers"] if Path(item["path"]).name == name]
    if len(matches) != 1:
        raise ValueError("one exact registered driver is required: " + name)
    binding = matches[0]
    source = check(binding)
    specification = importlib.util.spec_from_file_location("thruline_bound_" + name.replace(".", "_"), binding["path"])
    module = importlib.util.module_from_spec(specification)
    # Compile the checked bytes, avoiding a second unbound file read or a
    # dependence on sys.path when the broker runs as an embedded -c program.
    exec(compile(source, binding["path"], "exec"), module.__dict__)
    return module


def selected(name):
    path = Path(name)
    if path.is_absolute() or ".." in path.parts or ".git" in path.parts or not path.parts:
        raise ValueError("unsafe selected source path")
    return path.as_posix()


def yaml_suggestion(content):
    """Strict native subset, never repair/rewrite invalid model YAML.

    Quoted single-line scalars and literal blocks are supported. Complex YAML,
    multiple suggestions, aliases and uncertain line endings are rejected.
    """
    if not content.startswith("```yaml\ncode_suggestions:\n") or not content.rstrip().endswith("```"):
        raise ValueError("native suggestion representation unsupported")
    lines = content.rstrip()[:-3].splitlines()[2:]
    values = {}
    index = 0
    while index < len(lines):
        line = lines[index]
        if not line.strip():
            index += 1
            continue
        match = re.fullmatch(r"(- |  )([a-z_]+): (.*)", line)
        if not match or (not values and match[1] != "- ") or (values and match[1] != "  "):
            raise ValueError("native YAML requires exactly one simple suggestion")
        key, value = match[2], match[3]
        if key in values:
            raise ValueError("duplicate native YAML field")
        index += 1
        if value in ("|", "|-"):
            block = []
            while index < len(lines) and (lines[index].startswith("    ") or not lines[index]):
                block.append(lines[index][4:] if lines[index] else "")
                index += 1
            value = "\n".join(block).rstrip("\n") + ("\n" if value == "|" else "")
        elif value.startswith("'") and value.endswith("'"):
            interior = value[1:-1]
            if re.search(r"(?<!')'(?!')", interior):
                raise ValueError("unsupported native YAML quote escape")
            value = interior.replace("''", "'")
        elif value.startswith('"'):
            value = json.loads(value)
        else:
            raise ValueError("native YAML scalar must be quoted or a literal block")
        if not isinstance(value, str):
            raise ValueError("native suggestion field is not a string")
        values[key] = value
    if not {"relevant_file", "existing_code", "improved_code"} <= values.keys():
        raise ValueError("native suggestion omits required replacement fields")
    return values


class Broker:
    def __init__(self, request):
        self.request = request
        self.root = Path(request["root"]).resolve(strict=True)
        self.store = Path(request["store"]).resolve(strict=True)
        self.attempt = Path(request["attempt"]).resolve(strict=True)
        self.config = request["config"]
        self.recipe = request["recipe"]
        self.policy = self.config["recipes"][self.recipe]
        self.commands = []
        self.deadline = time.monotonic() + 650
        self.git_env = {"PATH": "/usr/bin:/bin", "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
                        "GIT_CONFIG_SYSTEM": "/dev/null", "GIT_TERMINAL_PROMPT": "0", "GIT_OPTIONAL_LOCKS": "0"}

    def run(self, argv, cwd, label, sandbox=False, read_paths=(), timeout=120, owned_native=False):
        if time.monotonic() >= self.deadline:
            raise TimeoutError("agent proposal action exhausted its time budget")
        output = self.attempt / f"{len(self.commands):03d}-{label}"
        output.mkdir()
        actual = list(argv)
        if Path(actual[0]).name in ("python", "python3", "python3.12", "python3.13"):
            actual[1:1] = ["-B", "-X", "pycache_prefix=/dev/null"]
        env = {**self.git_env, "PYTHONDONTWRITEBYTECODE": "1", "PYTHONPYCACHEPREFIX": "/dev/null", "PYTHONNOUSERSITE": "1", "TMPDIR": str(output)}
        if owned_native and self.policy.get('container_runtime'):
            # Only the verified invoke.py broker receives the product lease.
            # The native container receives neither lease values nor a socket.
            for name in ['THRULINE_BROKER_CONTAINER_NAME', 'THRULINE_BROKER_CONTAINER_OWNER',
                         'THRULINE_BROKER_MEMORY_MB', 'THRULINE_BROKER_CPU_SECONDS']:
                env[name] = os.environ[name]
        if sandbox:
            profile = ['(version 1)', '(allow default)', '(deny network*)', '(deny file-write*)',
                       '(deny file-read* (subpath ' + json.dumps(str(Path.home())) + '))',
                       '(allow file-write* (literal "/dev/null") (subpath ' + json.dumps(str(output)) + '))']
            for path in [cwd, *read_paths]:
                profile.append('(allow file-read* (subpath ' + json.dumps(str(path)) + '))')
            policy_path = output / "sandbox.sb"
            policy_path.write_text("\n".join(profile) + "\n")
            actual = ["/usr/bin/sandbox-exec", "-f", str(policy_path), *actual]
        started = time.monotonic()
        with (output / "stdout").open("wb") as out, (output / "stderr").open("wb") as err:
            child = subprocess.Popen(actual, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err)
            while child.poll() is None:
                if time.monotonic() - started > timeout or time.monotonic() >= self.deadline:
                    child.kill(); child.wait()
                    raise TimeoutError(label + " timed out; outer worker owns descendant termination")
                if out.tell() + err.tell() > 16 * 1024 * 1024:
                    child.kill(); child.wait()
                    raise ValueError(label + " exceeded output budget")
                time.sleep(.05)
        stdout = safe(output / "stdout")
        stderr = safe(output / "stderr")
        record = {"argv": actual, "cwd": str(cwd), "exit_code": child.returncode,
                  "duration_seconds": time.monotonic() - started, "stdout": ref(output / "stdout"),
                  "stderr": ref(output / "stderr"), "sandbox": sandbox,
                  "sandbox_profile": ref(policy_path) if sandbox else None}
        self.commands.append(record)
        write(self.attempt / "commands.json", self.commands)
        return child.returncode, stdout, stderr

    def git(self, args, cwd=None):
        code, stdout, stderr = self.run(["/usr/bin/git", "-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false",
                                        "-c", "submodule.recurse=false", "--no-pager", *args], cwd or self.root, "git", timeout=20)
        if code:
            raise ValueError("fixed Git operation failed: " + stderr.decode(errors="replace")[:1000])
        return stdout

    def configured(self):
        if self.config.get("schema") != "thruline.agent-repair-config.v1":
            raise ValueError("unsupported agent repair configuration")
        required = {"invoke.py", "engine.py", "model_identity.py", "runtime_identity.py", "process_ownership.py"}
        if not required <= {Path(item["path"]).name for item in self.config.get("drivers", [])}:
            raise ValueError("native driver dependency closure is not pinned")
        for binding in [self.policy["oracle"], *self.config.get("drivers", [])]:
            path = Path(binding["path"])
            if path.is_relative_to(self.root) or path.is_relative_to(self.store):
                raise ValueError("protected oracle/driver must be outside the reviewed project/stages")
            check(binding)
        ids = self.policy["oracle"].get("case_ids", [])
        if not ids or any(not isinstance(value, str) or not value for value in ids) or len(ids) != len(set(ids)):
            raise ValueError("frozen independent oracle case IDs are required")
        self.oracle_config = json.loads(check(self.policy["oracle"]))
        if self.oracle_config.get("schema") != "thruline.oracle-command.v1":
            raise ValueError("unsupported oracle command configuration")
        for binding in self.oracle_config["bound_files"]:
            if Path(binding["path"]).is_relative_to(self.root) or Path(binding["path"]).is_relative_to(self.store):
                raise ValueError("oracle input is writable by reviewed work")
            check(binding)

    def snapshot(self, root, names):
        rows = {}
        total = 0
        for name in names:
            path = root / selected(name)
            data = safe(path, root, 8 * 1024 * 1024)
            total += len(data)
            if total > 8 * 1024 * 1024:
                raise ValueError("selected source exceeds 8 MiB")
            rows[name] = {"sha256": sha(data), "size": len(data), "mode": path.stat().st_mode & 0o777}
        return rows

    def load_capture(self):
        capture = json.loads(check(self.request["capture"]))
        if capture.get("schema") != "thruline.local-agent-capture.v1" or capture["source"]["root"] != str(self.root):
            raise ValueError("native capture belongs to another source or schema")
        expected_id = {"aider": "R12", "openhands-sdk": "R13", "pr-agent": "R14", "mini-swe-agent": "R15"}[self.recipe]
        if capture.get("repository_id") != expected_id or capture["source"].get("original_unchanged") is not True:
            raise ValueError("native capture has mismatched provider or unestablished original source")
        if capture["source"].get("generated_outside_selected_scope"):
            raise ValueError("native proposal generated files outside its selected scope")
        if self.git(["rev-parse", "HEAD"]).decode().strip() != capture["source"]["head"]:
            raise ValueError("native proposal source HEAD is stale")
        names = capture["source"]["selected_files"]
        if not 1 <= len(names) <= 128 or len(names) != len(set(names)):
            raise ValueError("invalid native selected source denominator")
        snapshot = self.snapshot(self.root, names)
        before = {item["path"]: item for item in capture["source"]["before"]}
        if set(before) != set(names) or any(not before[name].get("exists") or before[name]["sha256"] != snapshot[name]["sha256"] for name in names):
            raise ValueError("native proposal base changed; dirty/removed inputs require a fresh proposal")
        for binding in capture["native_artifacts"]:
            check(binding)
        check(capture["proposal"])
        if base64.b64decode(capture["proposal"]["bytes_base64"], validate=True) != check(capture["proposal"]):
            raise ValueError("inline patch differs from native bytes")
        return capture, names, snapshot

    def oracle(self, workspace, label):
        self.configured()
        before = self.snapshot(workspace, self.names)
        argv = [str(value).replace("{workspace}", str(workspace)) for value in self.oracle_config["argv"]]
        read_paths = [Path(item["path"]) for item in self.oracle_config["bound_files"]]
        if self.policy.get("container_runtime"):
            manifest = self.policy["container_runtime"]; check(manifest)
            driver = next(item for item in self.config["drivers"] if Path(item["path"]).name == "container_native.py"); check(driver)
            spec = importlib.util.spec_from_file_location("thruline_protected_container_oracle", driver["path"])
            module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
            output = self.attempt / f"{len(self.commands):03d}-{label}"; output.mkdir()
            completed, actual, namespace = module.run_oracle(Path(manifest["path"]), workspace, output,
                "before" if label == "oracle-base" else "after", argv, self.oracle_config["bound_files"])
            code, stdout, stderr = completed.returncode, completed.stdout, completed.stderr
            (output / "stdout").write_bytes(stdout); (output / "stderr").write_bytes(stderr)
            self.commands.append({"argv": actual, "cwd": str(workspace), "exit_code": code,
                "stdout": ref(output / "stdout"), "stderr": ref(output / "stderr"), "sandbox": True,
                "namespace_command": namespace, "container_runtime": manifest})
            write(self.attempt / "commands.json", self.commands)
        else:
            code, stdout, stderr = self.run(argv, workspace, label, sandbox=True, read_paths=read_paths, timeout=min(self.oracle_config.get("timeout_seconds", 60), 120))
        report = json.loads(stdout)
        cases = report.get("checks", [])
        expected_ids = self.policy["oracle"]["case_ids"]
        ids = [case.get("check") for case in cases]
        if sorted(ids) != sorted(expected_ids) or len(ids) != len(set(ids)) or any(type(case.get("passed")) is not bool for case in cases):
            raise ValueError("independent oracle case identities differ from frozen expectation")
        passed = sum(case["passed"] for case in cases)
        if type(report.get("passed")) is not int or report.get("passed") != passed or report.get("total") != len(expected_ids):
            raise ValueError("independent oracle exact counts are inconsistent")
        if self.snapshot(workspace, self.names) != before:
            raise ValueError("oracle changed staged source")
        return {"passed": code == 0 and passed == len(expected_ids) and not stderr, "exit_code": code,
                "report": report, "source": before, "oracle_config_sha256": self.policy["oracle"]["sha256"],
                "command": self.commands[-1]}

    def preview(self, resume=False):
        capture, self.names, snapshot = self.load_capture()
        identity = sha(encoded({"capture": self.request["capture"], "source": snapshot, "config": self.config,
                                "candidate": self.request["candidate_sha256"], "nonce": uuid.uuid4().hex}))
        directory = self.store / "proposals" / identity
        directory.mkdir(parents=True)
        self.proposal_directory = directory
        base, stage = directory / "base", directory / "stage"
        base.mkdir(); stage.mkdir()
        for name in self.names:
            data = safe(self.root / name, self.root)
            for root in (base, stage):
                path = root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
                path.chmod(snapshot[name]["mode"])
        meta = {"schema": "thruline.agent-repair.v1", "digest": identity, "recipe": self.recipe, "state": "previewing",
                "root": str(self.root), "source_head": capture["source"]["head"], "base_snapshot": snapshot,
                "selected_files": self.names, "editable_files": capture["source"].get("editable_files", self.names),
                "capture": self.request["capture"], "source_binding_id": self.request["source_binding_id"],
                "config_sha256": sha(encoded(self.config)), "candidate_sha256": self.request["candidate_sha256"],
                "history": [], "limitations": ["Only the named frozen oracle criteria are evaluated; model completion is advisory.",
                                                "Selected files only. Other application behavior remains outside this repair."]}
        write(directory / "meta.json", meta)
        if self.recipe == "pr-agent":
            responses = next(item for item in capture["native_artifacts"] if item["relative_path"] == "native/model-responses.jsonl")
            content = json.loads(check(responses).splitlines()[0])["choices"][0]["message"]["content"]
            suggestion = yaml_suggestion(content)
            name = selected(suggestion["relevant_file"])
            if name not in meta["editable_files"]:
                raise ValueError("native suggestion changes a file outside accepted edit scope")
            old, new = suggestion["existing_code"].encode(), suggestion["improved_code"].encode()
            original = safe(stage / name, stage)
            if not old or old == new or original.count(old) != 1:
                raise ValueError("native suggestion is stale or not uniquely applicable")
            (stage / name).write_bytes(original.replace(old, new, 1))
            meta["native_suggestion"] = {"value": suggestion, "response": responses, "jsonl_line": 1,
                                         "json_pointer": "/choices/0/message/content", "application": "exact unique byte substitution"}
        else:
            native_workspace = Path(capture["invocation_path"]).parent.parent / "workspace"
            after = {item["path"]: item for item in capture["source"]["after"]}
            for name in self.names:
                data = safe(native_workspace / name, native_workspace)
                if not after[name].get("exists") or sha(data) != after[name]["sha256"]:
                    raise ValueError("native proposal workspace changed or deleted source")
                if name not in meta["editable_files"] and sha(data) != snapshot[name]["sha256"]:
                    raise ValueError("native proposal changed protected context")
                (stage / name).write_bytes(data)
            if resume:
                if self.recipe != "openhands-sdk":
                    raise ValueError("persisted resume is available only for OpenHands")
                options_driver = load_bound_driver(self.config, "openhands_options.py")
                openhands_options = options_driver.validate(capture.get("openhands_options"))
                if openhands_options["backend"] != "local":
                    raise ValueError("owned-server persistence is retained, but this product resume action requires a local SDK session")
                native = Path(capture["invocation_path"]).parent
                cid_ref = next(item for item in capture["native_artifacts"] if item["relative_path"] == "native/conversation-id.txt")
                cid = str(uuid.UUID(check(cid_ref).decode().strip()))
                persisted = [item for item in capture["native_artifacts"] if item["relative_path"].startswith("native/persistence/")]
                if not persisted:
                    raise ValueError("native conversation persistence was not retained")
                self.git(["init", "--quiet"], stage)
                self.git(["add", "--all"], stage)
                self.git(["-c", "user.name=ThruLine", "-c", "user.email=local@localhost", "commit", "-qm", "Bound resumed proposal"], stage)
                task = check(self.policy["resume_task"])
                task_path = directory / "resume-task.txt"; task_path.write_bytes(task)
                driver = next(item for item in self.config["drivers"] if Path(item["path"]).name == "invoke.py")
                check(driver); check(self.policy["model"])
                resume_output = directory / "native-resume"
                starting = self.snapshot(stage, self.names)
                argv = [self.config["python"]["path"], driver["path"], "R13", "--workspace", str(stage),
                        "--output", str(resume_output), "--task-file", str(task_path), "--model-provenance", self.policy["model"]["path"],
                        "--resume-id", cid, "--resume-from", str(native), "--worker-owned-group", "--iterations", "8", "--files", *meta["editable_files"]]
                if self.policy.get('container_runtime'):
                    check(self.policy['container_runtime'])
                    argv += ['--container-runtime', self.policy['container_runtime']['path']]
                argv += ["--openhands-json", json.dumps(openhands_options, separators=(",", ":"))]
                code, _, _ = self.run(argv, stage, "native-resume", timeout=630, owned_native=True)
                # The native wrapper independently revalidates runtime/model and
                # enters the registered Linux namespace or macOS sandbox.
                for item in capture["native_artifacts"]:
                    check(item)
                meta["resume"] = {"conversation_id": cid, "prior_workspace": str(native_workspace),
                                  "workspace": str(stage), "starting_source": starting, "exit_code": code,
                                  "persistence_sources": persisted, "invocation": ref(resume_output / "invocation.json"),
                                  "events": ref(resume_output / "events.jsonl"),
                                  "native_state": read(resume_output / "native-state.json") if (resume_output / "native-state.json").exists() else None,
                                  "prior_artifacts_unchanged": True}
                meta["resume"]["native_artifacts"] = [ref(path) for path in sorted(resume_output.rglob('*')) if path.is_file()]
                events = [json.loads(line) for line in safe(resume_output / "events.jsonl").splitlines()]
                meta["resume"]["action_count"] = sum(event.get("kind") == "ActionEvent" for event in events)
                meta["resume"]["observation_count"] = sum(event.get("kind") == "ObservationEvent" for event in events)
                meta["resume"]["native_completed"] = code == 0 and meta["resume"]["action_count"] > 0 and meta["resume"]["native_state"] is not None and meta["resume"]["native_state"].get("execution_status") == "finished"
                resume_mode = self.policy.get('resume_mode', 'verify_unchanged')
                if resume_mode not in ['verify_unchanged', 'continue_repair']:
                    raise ValueError('unknown protected native resume policy')
                meta['resume']['mode'] = resume_mode
                if resume_mode == 'verify_unchanged' and self.snapshot(stage, self.names) != starting:
                    meta["resume"]["source_changed_during_verification_task"] = True
                    meta["resume"]["native_completed"] = False
                    meta["limitations"].append("The configured resume task required unchanged source; this continuation changed it and is not a completed verification task.")
        for path in stage.rglob('*'):
            if '.git' in path.relative_to(stage).parts:
                continue
            if path.is_symlink() or (path.is_file() and path.relative_to(stage).as_posix() not in self.names):
                raise ValueError("unexpected file or symlink generated outside selected stage inputs")
        stage_snapshot = self.snapshot(stage, self.names)
        changed = [name for name in self.names if stage_snapshot[name]["sha256"] != snapshot[name]["sha256"]]
        if set(changed) - set(meta["editable_files"]):
            raise ValueError("proposal changes a protected context file")
        diff = "".join("".join(difflib.unified_diff(safe(base / name).decode().splitlines(True), safe(stage / name).decode().splitlines(True),
                                                 fromfile="a/" + name, tofile="b/" + name)) for name in changed)
        (directory / "proposal.diff").write_text(diff)
        meta.update(state="previewed", changed_files=changed, diff=diff, stage_snapshot=stage_snapshot,
                    proposal=ref(directory / "proposal.diff"), original_unchanged=self.snapshot(self.root, self.names) == snapshot)
        if not meta["original_unchanged"]:
            raise ValueError("original project changed during proposal capture")
        if resume and not meta["resume"]["native_completed"]:
            meta["state"] = "resume_incomplete"
        meta["history"].append({"action": "resume" if resume else "preview", "attempt": str(self.attempt), "state": meta["state"]})
        write(directory / "meta.json", meta)
        return {**meta, "meta_sha256": sha(encoded(meta))}

    def proceed(self):
        identity = self.request["digest"]
        if not re.fullmatch(r"[a-f0-9]{64}", identity):
            raise ValueError("invalid proposal identity")
        directory = self.store / "proposals" / identity
        path = directory / "meta.json"
        raw = safe(path, self.store)
        if sha(raw) != self.request.get("prior_meta_sha256"):
            raise ValueError("proposal metadata is missing, changed or not bound by a prior receipt")
        meta = json.loads(raw); self.names = meta["selected_files"]
        try:
            if meta["root"] != str(self.root) or meta["recipe"] != self.recipe or meta["candidate_sha256"] != self.request["candidate_sha256"]:
                raise ValueError("proposal belongs to a different root, executor or product candidate")
            if meta["config_sha256"] != sha(encoded(self.config)) or self.snapshot(self.root, self.names) != meta["base_snapshot"]:
                raise ValueError("proposal base or protected oracle/configuration changed")
            if self.git(["rev-parse", "HEAD"]).decode().strip() != meta["source_head"]:
                raise ValueError("proposal base HEAD changed")
            base, stage = directory / "base", directory / "stage"
            if self.snapshot(base, self.names) != meta["base_snapshot"] or self.snapshot(stage, self.names) != meta["stage_snapshot"]:
                raise ValueError("staged or preserved source bytes changed")
            check(meta["proposal"])
            action = self.request["action"]
            if action == "verify":
                if meta["state"] not in ("previewed", "verification_failed"):
                    raise ValueError("only a completed captured proposal may enter independent verification")
                before = self.oracle(base, "oracle-base")
                after = self.oracle(stage, "oracle-stage")
                regressions = [case["check"] for case in before["report"]["checks"] if case["passed"] and
                               not next(value["passed"] for value in after["report"]["checks"] if value["check"] == case["check"])]
                meta["verification"] = {"before": before, "after": after, "regressions": regressions}
                meta["state"] = "verified_for_named_checks" if not before["passed"] and after["passed"] and not regressions and meta["changed_files"] else "verification_failed"
            elif action == "apply":
                if meta["state"] != "verified_for_named_checks":
                    raise ValueError("independent frozen oracle must pass before applying")
                # Narrow exact writes use no patch-path interpretation. Capture
                # all originals first; retain any unexpected partial failure.
                changed = meta["changed_files"]
                if set(changed) - set(meta["editable_files"]):
                    raise ValueError("apply scope exceeds accepted edit scope")
                meta["apply_originals"] = {name: ref(self.root / name) for name in changed}
                for name in changed:
                    data = safe(stage / name, stage)
                    target = self.root / name
                    if sha(safe(target, self.root)) != meta["base_snapshot"][name]["sha256"]:
                        raise ValueError("source changed immediately before exact apply")
                    # Open every directory without following symlinks and bind
                    # the bytes again through the writable descriptor itself.
                    parent_fd = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
                    try:
                        for component in Path(name).parts[:-1]:
                            next_fd = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=parent_fd)
                            os.close(parent_fd); parent_fd = next_fd
                        descriptor = os.open(Path(name).name, os.O_RDWR | os.O_NOFOLLOW, dir_fd=parent_fd)
                    finally:
                        os.close(parent_fd)
                    with os.fdopen(descriptor, "r+b") as stream:
                        if sha(stream.read(8 * 1024 * 1024 + 1)) != meta["base_snapshot"][name]["sha256"]:
                            raise ValueError("source changed through the opened apply descriptor")
                        stream.seek(0); stream.write(data); stream.truncate(); stream.flush(); os.fsync(stream.fileno())
                if self.snapshot(self.root, self.names) != meta["stage_snapshot"]:
                    raise ValueError("applied source differs from verified candidate; inspect preserved originals")
                meta["state"] = "applied_verified_patch"
            else:
                raise ValueError("unsupported proposal action")
        except Exception as error:
            meta.update(state="rejected", error=str(error))
        meta["history"].append({"action": self.request["action"], "attempt": str(self.attempt), "state": meta["state"], "error": meta.get("error")})
        write(path, meta)
        return {**meta, "meta_sha256": sha(encoded(meta))}


def main():
    request = read(Path(sys.argv[1]))
    broker = Broker(request)
    with (broker.store / "agent-repair.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            broker.configured()
            action = request["action"]
            value = broker.preview(resume=action == "resume") if action in ("preview", "resume") else broker.proceed()
        except Exception as error:
            value = {"schema": "thruline.agent-repair.v1", "state": "rejected", "error": str(error),
                     "root": str(broker.root), "recipe": broker.recipe, "digest": request["digest"], "attempt": str(broker.attempt)}
        # Snapshot the exact completed/failed action closure before mutable
        # result metadata is written. The Rust publisher copies these bytes to
        # its managed content-addressed store; original paths are provenance.
        try:
            roots = [broker.attempt]
            if hasattr(broker, "proposal_directory"):
                roots.append(broker.proposal_directory)
            artifacts = []
            total = 0
            for root in roots:
                for path in sorted(root.rglob("*")):
                    if path.name in ("result.json", "transport.json") or ".git" in path.relative_to(root).parts:
                        continue
                    if path.is_symlink():
                        raise ValueError("symlink in attempted action artifact closure")
                    if path.is_file():
                        binding = ref(path); total += binding["size"]
                        if len(artifacts) >= 4096 or total > 128 * 1024 * 1024:
                            raise ValueError("action artifact closure exceeds capture bound")
                        artifacts.append(binding)
            value["action_artifacts"] = artifacts
        except Exception as error:
            value["previous_state"] = value.get("state")
            value.update(state="capture_failed", artifact_capture_error=str(error))
        write(broker.attempt / "result.json", value)
        print(json.dumps(value))


if __name__ == "__main__":
    main()
