#!/usr/bin/env python3
"""Trusted bounded Ruff repair broker, embedded into the delivered ThruLine CLI.

Target content is data. Only pinned Ruff F401 safe fixes execute. The independent
oracle is configured outside the project and runs with source/oracle read-only.
The caller supplies resource limits, cancellation and command evidence binding.
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
import subprocess
import sys
import time
import uuid


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def write(path, value):
    temporary = path.with_name(path.name + ".new")
    with temporary.open("wb") as out:
        out.write(encoded(value))
        out.flush()
        os.fsync(out.fileno())
    temporary.replace(path)


def read(path):
    if path.is_symlink() or path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError("unsafe or over-budget artifact")
    return json.loads(path.read_bytes())


def below(path, root):
    return path == root or root in path.parents


class Broker:
    def __init__(self, request):
        self.request = request
        self.root = Path(request["root"]).resolve(strict=True)
        self.store = Path(request["store"]).resolve(strict=True)
        self.attempt = Path(request["attempt"]).resolve(strict=True)
        self.config = request["config"]
        self.commands = []
        self.deadline = time.monotonic() + 220

    def run(self, argv, cwd, label, sandbox=False, writable=False):
        label = f"{len(self.commands):03d}-{label}"
        if time.monotonic() > self.deadline:
            raise TimeoutError("repair exceeded total execution budget")
        actual = argv
        if sandbox:
            if sys.platform != "darwin":
                raise ValueError("read-only independent oracle sandbox unavailable on this platform")
            # Pinned runtime and oracle are trusted setup; reviewed source may be hostile.
            # No host home reads, writes outside disposable scratch, or network access.
            scratch = self.attempt / (label + ".scratch")
            scratch.mkdir()
            # HOME is an environment setting, not an authority boundary. Deny
            # all unselected reads even when the caller supplies a private HOME.
            allow_read = [Path(cwd), scratch, Path(self.config["oracle"]["path"]), Path(self.config["ruff"]["path"]), Path(self.config["python"]["path"])]
            allow_read += [Path(p) for p in ["/System","/usr/bin","/usr/lib","/usr/share","/bin","/sbin","/dev","/Library/Frameworks/Python.framework","/private/etc/localtime","/private/etc/passwd","/private/etc/group"] if Path(p).exists()]
            profile = '(version 1)(allow default)(deny network*)(deny file-write*)(deny file-read*)(allow file-read-metadata)(allow file-read* (literal "/"))'
            for item in allow_read:
                path=item.resolve(strict=True); kind="subpath" if path.is_dir() else "literal"
                profile += '(allow file-read* ('+kind+' '+json.dumps(str(path),ensure_ascii=False)+'))'
                for parent in path.parents: profile += '(allow file-read* (literal '+json.dumps(str(parent),ensure_ascii=False)+'))'
            profile += '(allow file-write* (subpath '+json.dumps(str(scratch),ensure_ascii=False)+') (literal "/dev/null"))'
            if writable:
                if self.config.get("schema")!="thruline.ruff-repair-config.v2":
                    profile += '(allow file-write* (subpath '+json.dumps(str(cwd),ensure_ascii=False)+'))'
                else:
                    for name in self.config["files"]: profile += '(allow file-write* (literal '+json.dumps(str(Path(cwd)/name),ensure_ascii=False)+'))'
            (self.attempt/(label+'.sb')).write_text(profile)
            actual = ["/usr/bin/sandbox-exec", "-p", profile, "--", *argv]
        stdout_path = self.attempt / (label + ".stdout")
        stderr_path = self.attempt / (label + ".stderr")
        env = {"PATH": "/usr/bin:/bin", "PYTHONNOUSERSITE": "1", "PYTHONDONTWRITEBYTECODE": "1",
               "TMPDIR": str(self.attempt / (label + ".scratch")) if sandbox else str(self.attempt)}
        started = time.monotonic()
        # Files are retained while the outer worker bounds RSS, CPU and wall time.
        with stdout_path.open("wb") as out, stderr_path.open("wb") as err:
            child = subprocess.Popen(actual, cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=out, stderr=err,
                                     preexec_fn=lambda: resource.setrlimit(resource.RLIMIT_CPU, (30, 30)))
            while child.poll() is None:
                if time.monotonic() - started > 45 or time.monotonic() > self.deadline:
                    child.kill()
                    child.wait()
                    raise TimeoutError(label + " timed out")
                if stdout_path.stat().st_size + stderr_path.stat().st_size > 8 * 1024 * 1024:
                    child.kill()
                    child.wait()
                    raise ValueError(label + " exceeded output budget")
                time.sleep(0.02)
        if stdout_path.stat().st_size + stderr_path.stat().st_size > 8 * 1024 * 1024:
            raise ValueError(label + " exceeded output budget at exit")
        result = {"label": label, "argv": argv, "cwd": str(cwd), "exit_code": child.returncode,
                  "seconds": time.monotonic() - started, "cpu_seconds": 30,
                  "sandbox": "no-network; restricted home reads; bounded writes" if sandbox else "trusted embedded broker; fixed local Git operation; hooks disabled",
                  "stdout_sha256": sha(stdout_path.read_bytes()), "stderr_sha256": sha(stderr_path.read_bytes())}
        self.commands.append(result)
        write(self.attempt / "commands.json", self.commands)
        return child.returncode, stdout_path.read_bytes(), stderr_path.read_bytes()

    def git(self, argv, cwd=None, label=None):
        code, out, err = self.run(["/usr/bin/git", "-c", "core.hooksPath=/dev/null", "-c", "submodule.recurse=false", *argv], cwd or self.root, label or f"git-{len(self.commands)}")
        if code:
            raise ValueError("Git operation failed: " + err.decode(errors="replace")[:1000])
        return out

    def snapshot(self, root, label):
        names = self.git(["ls-files", "--cached", "--others", "--exclude-standard", "-z"], root, label).split(b"\0")
        names = sorted(set(name.decode() for name in names if name))
        if len(names) > 10000:
            raise ValueError("repair selection exceeds 10,000 files")
        files, total = {}, 0
        for name in names:
            relative = Path(name)
            if relative.is_absolute() or ".." in relative.parts or ".git" in relative.parts:
                raise ValueError("invalid source path")
            path = root / relative
            if path.is_symlink() or any(p.is_symlink() for p in path.parents if below(p, root)):
                raise ValueError("symlink source cannot be safely staged: " + name)
            if not path.exists():
                files[name] = None
                continue
            if not path.is_file():
                raise ValueError("submodule or non-file source needs an explicit repair adapter: " + name)
            size = path.stat().st_size
            total += size
            if size > 8 * 1024 * 1024 or total > 64 * 1024 * 1024:
                raise ValueError("repair source exceeds 8 MiB/file or 64 MiB total")
            files[name] = {"sha256": sha(path.read_bytes()), "mode": path.stat().st_mode & 0o777}
        return {"files": files, "scope": "tracked and nonignored untracked files; ignored inputs excluded", "bytes": total}

    def repair_mode(self):
        return self.config.get("mode", "legacy-f401")

    def repair_rule(self):
        return {"legacy-f401":"F401", "imports":"I001", "format":"unformatted", "notebook-format":"unformatted"}[self.repair_mode()]

    def selected_files(self):
        return self.config.get("files", ["."])

    def notebook_preserved(self, before, after):
        old, new = json.loads(before), json.loads(after)
        if old.get("nbformat") != 4 or len(old.get("cells",[])) != len(new.get("cells",[])):
            raise ValueError("notebook schema/cell count changed")
        ids=set()
        for left,right in zip(old["cells"],new["cells"]):
            if not isinstance(left.get("id"),str) or not left["id"] or left["id"] in ids:
                raise ValueError("notebook cell identity is missing or ambiguous")
            ids.add(left["id"])
            if left.get("cell_type") == "code":
                text=''.join(left['source']) if isinstance(left['source'],list) else left['source']
                if any(line.lstrip().startswith(('%','!')) for line in text.splitlines()):
                    raise ValueError("notebook magic/shell execution remains unsupported")
                left["source"] = None; right["source"] = None
        if old != new:
            raise ValueError("native notebook formatter changed metadata, output, markdown or cell identities")
        language=old.get('metadata',{}).get('language_info',{}).get('name')
        kernel=old.get('metadata',{}).get('kernelspec',{}).get('language')
        if language not in (None,'python') or kernel not in (None,'python') or (language is None and kernel is None):
            raise ValueError("only explicitly Python notebooks are supported")

    def native(self, stage, label, fix=False):
        if self.repair_mode() != "legacy-f401":
            format_mode=self.repair_rule() == "unformatted"
            args=[self.config["ruff"]["path"],"format" if format_mode else "check","--isolated","--no-cache","--output-format","json"]
            if format_mode:
                if not fix: args.append("--check")
            else:
                args += ["--select","I001","--ignore-noqa","--no-respect-gitignore",*(["--fix","--no-unsafe-fixes"] if fix else ["--no-fix"])]
            args += self.selected_files()
            code,out,err=self.run(args,stage,label,sandbox=True,writable=fix)
            if code not in (0,1) or err or (fix and code != 0):
                raise ValueError("selected native Ruff mode did not complete; inspect retained stdout/stderr")
            if format_mode and fix:
                if out.strip(): raise ValueError("unexpected native formatter write output")
                return []
            rows=json.loads(out)
            if not isinstance(rows,list) or any(row.get("code") not in (self.repair_rule(),) for row in rows):
                raise ValueError("selected mode returned an unsupported or incomplete native diagnostic")
            return rows
        args = [self.config["ruff"]["path"], "check", "--isolated", "--select", "F401" if fix else "F401,F841",
                "--no-cache", "--ignore-noqa", "--no-respect-gitignore", "--output-format", "json",
                *( ["--fix", "--no-unsafe-fixes"] if fix else ["--no-fix"] ), "."]
        code, out, err = self.run(args, stage, label, sandbox=True, writable=fix)
        if code not in (0, 1) or err:
            raise ValueError("Ruff did not produce a complete supported report")
        rows = json.loads(out)
        if not isinstance(rows, list):
            raise ValueError("unrecognized Ruff diagnostics")
        return rows

    def configured(self):
        if self.config.get("schema") not in ("thruline.ruff-repair-config.v1", "thruline.ruff-repair-config.v2"):
            raise ValueError("unsupported repair configuration")
        if self.config["ruff"].get("version") != "0.16.7":
            raise ValueError("unsupported Ruff repair version")
        for name in ["python", "ruff", "oracle"]:
            item = self.config[name]
            path = Path(item["path"]).resolve(strict=True)
            if below(path, self.root) or below(path, self.store):
                raise ValueError(name + " must be outside the project and repair stages")
            if sha(path.read_bytes()) != item["sha256"]:
                raise ValueError(name + " identity changed")
        if self.config["schema"] == "thruline.ruff-repair-config.v2":
            if set(self.config) != {"schema","mode","files","python","ruff","oracle"} or self.repair_mode() not in ("imports","format","notebook-format"):
                raise ValueError("unsupported selected Ruff repair mode")
            files=self.selected_files()
            if not isinstance(files,list) or not 1 <= len(files) <= 128 or len(set(files)) != len(files):
                raise ValueError("select 1-128 unique files")
            suffixes={".ipynb"} if self.repair_mode()=="notebook-format" else {".py",".pyi"}
            for name in files:
                p=Path(name)
                if not isinstance(name,str) or p.is_absolute() or not p.parts or any(v in (".","..",".git") for v in p.parts) or p.suffix not in suffixes:
                    raise ValueError("unsupported repair source selection")
                path=self.root/p
                if path.is_symlink() or any(x.is_symlink() for x in path.parents if below(x,self.root)) or not path.is_file():
                    raise ValueError("selected source is missing or a symlink")
                if path.suffix==".ipynb": self.notebook_preserved(path.read_bytes(),path.read_bytes())
            selection=read(self.root/".thruline/ruff.json")
            expected_mode="format" if self.repair_rule()=="unformatted" else "imports"
            if selection != {"schema":"thruline.ruff-selection.v1","mode":expected_mode,"files":files,"configuration":"isolated"}:
                raise ValueError("external repair scope differs from selected isolated analysis policy")
        elif "mode" in self.config or "files" in self.config:
            raise ValueError("legacy F401 repair does not admit new mode fields")
        ids = self.config["oracle"]["case_ids"]
        if not isinstance(ids, list) or not ids or not all(isinstance(i, str) and i.strip() for i in ids) or len(ids) != len(set(ids)):
            raise ValueError("independent acceptance case IDs must be declared and nonempty")

    def oracle(self, stage, label):
        before = self.snapshot(stage, label + "-before")
        code, out, err = self.run([self.config["python"]["path"], "-I", self.config["oracle"]["path"], "--root", str(stage)], stage, label, sandbox=True)
        after = self.snapshot(stage, label + "-after")
        try:
            result = json.loads(out)
        except (ValueError,UnicodeDecodeError):
            if self.repair_mode()=="legacy-f401": raise
            return {"passed":False,"exit_code":code,"report":None,"error":"Selected independent oracle did not produce a complete report","stderr":err.decode(errors="replace")[:4000],"source_sha256":sha(encoded(before)),"oracle_sha256":self.config["oracle"]["sha256"]}
        ids = self.config["oracle"]["case_ids"]
        cases = result.get("cases", [])
        valid = result.get("schema") == "thruline.repair-oracle.v1" and sorted(c.get("id", "") for c in cases) == sorted(ids)
        passed = valid and all(c.get("status") == "passed" for c in cases)
        if not valid or result.get("total") != len(ids) or type(result.get("passed")) is not int or result.get("passed") != sum(c.get("status") == "passed" for c in cases) or result.get("failed") != sum(c.get("status") != "passed" for c in cases):
            raise ValueError("oracle response does not match accepted case identities/counts")
        if before != after:
            raise ValueError("independent oracle or target changed the candidate")
        return {"passed": code == 0 and passed and not err, "exit_code": code, "report": result,
                "source_sha256": sha(encoded(before)), "oracle_sha256": self.config["oracle"]["sha256"]}

    def preview(self):
        snapshot = self.snapshot(self.root, "source-before")
        head = self.git(["rev-parse", "HEAD"], label="base-commit").decode().strip()
        digest = sha(encoded({"snapshot": snapshot, "head": head, "config": self.config,
                              "candidate": self.request["candidate_sha256"], "nonce": uuid.uuid4().hex}))
        directory = self.store / "repairs" / digest
        directory.mkdir(parents=True)
        base, stage = directory / "base", directory / "stage"
        self.git(["worktree", "add", "--detach", "--no-checkout", str(stage), head], label="create-isolated-worktree")
        base.mkdir()
        for name, identity in snapshot["files"].items():
            if identity is not None:
                for target in [base / name, stage / name]:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(self.root / name, target)
        # Base uses its own index so the unchanged oracle sees the same selection.
        self.git(["init", "--quiet"], base, "initialize-base-index")
        self.git(["add", "--all"], base, "index-base-snapshot")
        rows = self.native(stage, "original-diagnostic")
        safe = [row for row in rows if row.get("code") == self.repair_rule() and (row.get("fix") or {}).get("applicability") == "safe"]
        withheld = [row for row in rows if row not in safe]
        if not safe:
            raise ValueError("No current safe remedy for the selected native mode was found; unrelated findings are not applied")
        expected = {}
        for row in safe:
            path = Path(row["filename"]).resolve(strict=True)
            if not below(path, stage):
                raise ValueError("diagnostic points outside isolated stage")
            relative = str(path.relative_to(stage))
            if self.repair_mode() == "notebook-format":
                if relative not in self.selected_files() or row.get("cell") is None:
                    raise ValueError("notebook formatting diagnostic lies outside selected code cells")
                expected[relative] = {"notebook": True}
                continue
            text = path.read_text()
            lines = text.splitlines(keepends=True)
            edits = expected.setdefault(relative, {"text": text, "edits": []})["edits"]
            for edit in row["fix"]["edits"]:
                positions = []
                for point in [edit["location"], edit["end_location"]]:
                    row_num, col = point["row"], point["column"]
                    if type(row_num) is not int or type(col) is not int or row_num < 1 or row_num > len(lines) + 1 or col < 1:
                        raise ValueError("invalid upstream edit span")
                    if row_num == len(lines) + 1 and col != 1:
                        raise ValueError("invalid terminal edit span")
                    if row_num <= len(lines) and col > len(lines[row_num - 1]) + 1:
                        raise ValueError("upstream edit exceeds source line")
                    positions.append(sum(map(len, lines[:row_num - 1])) + col - 1)
                edits.append((*positions, edit["content"]))
        for value in expected.values():
            if value.get("notebook"): continue
            edits = sorted(value["edits"])
            if any(end < start for start, end, _ in edits) or any(a[1] > b[0] for a, b in zip(edits, edits[1:])):
                raise ValueError("conflicting upstream safe fixes")
            text = value["text"]
            for start, end, content in reversed(edits):
                text = text[:start] + content + text[end:]
            value["expected"] = text
        self.native(stage, "native-safe-fix", fix=True)
        native_snapshot = self.snapshot(stage, "native-fix-scope")
        if set(native_snapshot["files"]) != set(snapshot["files"]):
            raise ValueError("native fix added or removed source outside captured edits")
        changed, patch = [], []
        for name, identity in snapshot["files"].items():
            if identity is None:
                continue
            if native_snapshot["files"][name] is None or native_snapshot["files"][name]["mode"] != identity["mode"]:
                raise ValueError("native fix removed source or changed its permissions")
            after = (stage / name).read_bytes()
            original = (base / name).read_bytes()
            if original != after:
                if name not in expected:
                    raise ValueError("native fix changed source beyond captured safe edits")
                if expected[name].get("notebook"):
                    self.notebook_preserved(original, after)
                elif after.decode() != expected[name]["expected"]:
                    raise ValueError("native fix changed source beyond captured safe edits")
                changed.append(name)
                patch.extend(difflib.unified_diff(original.decode().splitlines(keepends=True), after.decode().splitlines(keepends=True), "a/" + name, "b/" + name))
        if not changed:
            raise ValueError("native fix did not produce a patch")
        patch_text = "".join(patch)
        (directory / "change.patch").write_text(patch_text)
        if self.snapshot(self.root, "source-after-preview") != snapshot:
            raise ValueError("source changed during preview; preserved patch is stale")
        result = {"schema": "thruline.ruff-repair.v1", "state": "previewed", "digest": digest,
                  "root": str(self.root), "base_commit": head, "base_snapshot": snapshot,
                  "config_sha256": sha(encoded(self.config)), "candidate_sha256": self.request["candidate_sha256"],
                  "changed_files": changed, "diff": patch_text, "patch_sha256": sha(patch_text.encode()),
                  "mode":self.repair_mode(), "selected_rule":self.repair_rule(), "original_diagnostics": rows, "withheld": withheld, "stage_snapshot": self.snapshot(stage, "staged-source"),
                  "artifacts": str(directory), "history": [{"action": "preview", "attempt": str(self.attempt), "state": "previewed"}],
                  "limitations": ["Only captured native safe edits for " + self.repair_rule() + " are selected. Notebook formatting permits code-cell source changes only; metadata, outputs, markdown and IDs stay identical.",
                    "Safe is Ruff's classification; independent acceptance is required before applying to the selected project.",
                    snapshot["scope"], "Local evidence custody does not prove independent producer truth."]}
        write(directory / "repair.json", result)
        return {**result, "meta_sha256": sha(encoded(result))}

    def existing(self):
        digest = self.request["digest"]
        if not re.fullmatch("[0-9a-f]{64}", digest):
            raise ValueError("a current repair digest is required")
        directory = self.store / "repairs" / digest
        meta = read(directory / "repair.json")
        if meta["root"] != str(self.root) or meta["digest"] != digest:
            raise ValueError("repair belongs to another project")
        if sha(encoded(meta)) != self.request.get("prior_meta_sha256"):
            raise ValueError("repair state does not match its last captured command receipt")
        return directory, meta

    def check_current(self, directory, meta):
        if meta["config_sha256"] != sha(encoded(self.config)) or meta["candidate_sha256"] != self.request["candidate_sha256"]:
            raise ValueError("repair configuration, oracle or ThruLine candidate changed; create a fresh preview")
        if self.git(["rev-parse", "HEAD"], label="current-commit").decode().strip() != meta["base_commit"]:
            raise ValueError("Git base revision changed after preview")
        if self.snapshot(self.root, "current-source") != meta["base_snapshot"]:
            raise ValueError("source changed after preview; patch is stale and must be recomputed")
        if self.snapshot(directory / "stage", "current-stage") != meta["stage_snapshot"]:
            raise ValueError("isolated patch changed after preview")
        if sha((directory / "change.patch").read_bytes()) != meta["patch_sha256"]:
            raise ValueError("patch identity changed")

    def proceed(self, action):
        directory, meta = self.existing()
        meta.pop("error", None)
        try:
            self.check_current(directory, meta)
            if action == "verify":
                baseline = self.oracle(directory / "base", "baseline-acceptance")
                changed = self.oracle(directory / "stage", "changed-acceptance")
                rows = self.native(directory / "stage", "detector-rerun")
                self.configured()
                self.check_current(directory, meta)
                cleared = not any(row.get("code") == self.repair_rule() for row in rows)
                meta["verification"] = {"base": baseline, "changed": changed, "selected_rule":self.repair_rule(), "detector_cleared":cleared, "detector_cleared_F401":cleared if self.repair_rule()=="F401" else None, "remaining_diagnostics": rows}
                meta["state"] = "verified_for_named_checks" if baseline["passed"] and changed["passed"] and cleared else "verification_failed"
            elif action == "apply":
                if meta["state"] != "verified_for_named_checks":
                    raise ValueError("independent verification must pass before applying")
                self.git(["apply", "--check", str(directory / "change.patch")], label="apply-preflight")
                self.check_current(directory, meta)
                self.git(["apply", str(directory / "change.patch")], label="apply-verified-patch")
                after = self.snapshot(self.root, "applied-source")
                if after != meta["stage_snapshot"]:
                    raise ValueError("source changed during application; inspect preserved originals and patch")
                meta["state"] = "applied_verified_patch"
                meta["applied_snapshot"] = after
            else:
                raise ValueError("unsupported repair action")
        except Exception as error:
            meta["state"] = "rejected"
            meta["error"] = str(error)
        meta["history"].append({"action": action, "attempt": str(self.attempt), "state": meta["state"], "error": meta.get("error")})
        write(directory / "repair.json", meta)
        return {**meta, "meta_sha256": sha(encoded(meta))}

    def execute(self):
        self.configured()
        action = self.request["action"]
        if action == "preview":
            return self.preview()
        if action == "list":
            rows = []
            for path in sorted((self.store / "repairs").glob("*/repair.json"), key=lambda p: p.stat().st_mtime, reverse=True)[:100]:
                value = read(path)
                if value["root"] == str(self.root):
                    rows.append(value)
            return {"schema": "thruline.ruff-repair-list.v1", "repairs": rows}
        return self.proceed(action)


def main():
    request = read(Path(sys.argv[1]))
    broker = Broker(request)
    with (broker.store / "repair.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            value = broker.execute()
        except Exception as error:
            value = {"schema": "thruline.ruff-repair.v1", "state": "unavailable" if isinstance(error, FileNotFoundError) else "failed",
                     "error": str(error), "attempt": str(broker.attempt), "digest": request["digest"], "root": str(broker.root)}
        write(broker.attempt / "result.json", value)
        print(json.dumps(value))


if __name__ == "__main__":
    main()
