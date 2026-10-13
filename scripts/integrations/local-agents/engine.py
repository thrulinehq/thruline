#!/usr/bin/env python3
"""Thin native API drivers. Run through invoke.py for isolation and receipts."""
import argparse
import asyncio
import json
import os
from pathlib import Path
import sys
import uuid

p = argparse.ArgumentParser()
p.add_argument("repository", choices=["R12", "R13", "R14", "R15"])
p.add_argument("--workspace", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
p.add_argument("--task-file", type=Path, required=True)
p.add_argument("--model-name",default="qwen2.5-coder:7b")
p.add_argument("--local-model-port", type=int, choices=[11435, 11436], required=True,
               help="The explicit accepted guest mailbox port.")
p.add_argument("--feedback-dir", type=Path)
p.add_argument("--aider-json")
p.add_argument("--openhands-json")
p.add_argument("--mini-json")
p.add_argument("--pr-agent-json", help="Explicit native plain-diff provider and patch file.")
p.add_argument("--resume-id")
p.add_argument("--conversation-id", help="Optional operator-bound UUID for a new OpenHands conversation")
p.add_argument("--interrupt-after-observations", type=int)
p.add_argument("--iterations", type=int, default=8)
p.add_argument("--command", default="review", choices=["review", "improve", "describe"])
p.add_argument("--files", nargs="+", default=["notes.py"])
a = p.parse_args()
model_base = "http://127.0.0.1:" + str(a.local_model_port)
if a.aider_json and (a.repository != 'R12' or os.environ.get('THRULINE_NATIVE_NAMESPACE') != 'linux'):
    raise SystemExit('native Aider modes require the pinned Linux runtime')
if a.feedback_dir and (a.repository != 'R12' or os.environ.get('THRULINE_NATIVE_NAMESPACE') != 'linux' or a.feedback_dir != Path('/feedback')):
    raise SystemExit('native feedback requires the pinned Aider feedback mount')
if a.interrupt_after_observations is not None and (a.repository != 'R13' or not 1 <= a.interrupt_after_observations <= 20 or a.resume_id):
    raise SystemExit('Native observation interruption is only available on a fresh OpenHands run with a bounded count')
if a.conversation_id and (a.repository != 'R13' or a.resume_id):
    raise SystemExit('conversation-id requires a fresh native OpenHands conversation')
if a.openhands_json and (a.repository != 'R13' or os.environ.get('THRULINE_NATIVE_NAMESPACE') != 'linux'):
    raise SystemExit('native OpenHands options require the pinned Linux runtime')
if a.pr_agent_json and (a.repository != 'R14' or os.environ.get('THRULINE_NATIVE_NAMESPACE') != 'linux'):
    raise SystemExit('native plain-diff requires PR-Agent in the pinned Linux runtime')
if a.mini_json and (a.repository != 'R15' or os.environ.get('THRULINE_NATIVE_NAMESPACE') != 'linux'):
    raise SystemExit('mini-SWE harness requires the pinned Linux runtime')
task = a.task_file.read_text()
os.chdir(a.workspace)

if a.repository == "R12":
    from aider.main import main
    if os.environ.get('THRULINE_NATIVE_NAMESPACE') == 'linux':
        from aider.repomap import RepoMap
        # Keep the native map cache out of selected source, while preserving the
        # actual RepoMap implementation and its emitted context.
        RepoMap.TAGS_CACHE_DIR = str(a.output / 'repository-map-cache')
    from aider_modes import validate as validate_aider_mode, native_options, run_mode
    mode_config = validate_aider_mode(json.loads(a.aider_json)) if a.aider_json else validate_aider_mode(None)
    if mode_config['mode'] == 'ask' and a.feedback_dir:raise ValueError('Ask mode cannot run repair feedback')
    (a.output/'aider-mode-config.json').write_text(json.dumps(mode_config,indent=2)+'\n')
    feedback_mode = None
    feedback_args = ['--no-auto-lint', '--no-auto-test']
    if a.feedback_dir:
        from feedback import guest as feedback_guest
        feedback_mode, feedback_argv, feedback_command = feedback_guest(a.feedback_dir)
        feedback_args = ['--auto-lint' if feedback_mode == 'lint-and-test' else '--no-auto-lint', '--auto-test', '--test-cmd', feedback_command]
        (a.output/'feedback-native-config.json').write_text(json.dumps({'mode':feedback_mode,'argv':feedback_argv,'command':feedback_command,'scope':'Native development feedback; independent acceptance remains separate.'},indent=2)+'\n')
    args = ["--model", "openai/"+a.model_name, "--openai-api-base", model_base+"/v1",
            "--openai-api-key", "local-no-secret", *native_options(mode_config,a.model_name), "--no-stream",
            "--no-auto-commits", "--no-dirty-commits", "--no-gitignore", "--no-analytics", "--no-check-update",
            "--no-show-model-warnings", "--yes-always", *feedback_args, "--map-tokens", "1024", "--map-refresh", "always",
            "--input-history-file", str(a.output / "input-history.txt"),
            "--chat-history-file", str(a.output / "chat-history.md"),
            "--llm-history-file", str(a.output / "llm-history.md"),
            "--env-file", str(a.output / "empty.env"), "--config", str(a.output / "empty.yml"),
            "--message-file", str(a.task_file), *a.files]
    (a.output / "aider-argv.json").write_text(json.dumps(args, indent=2))
    coder=main(args,return_coder=True)
    if not hasattr(coder,'run'):
        raise SystemExit(coder if isinstance(coder,int) else 3)
    coder.show_announcements()
    native_map=coder.get_repo_map(force_refresh=True)
    (a.output/'repository-map.txt').write_text(native_map or '')
    (a.output/'repository-map-metadata.json').write_text(json.dumps({'native_method':'Coder.get_repo_map(force_refresh=True)','map_tokens':1024,'refresh':'always','nonempty':bool(native_map),'editable_files':a.files},indent=2)+'\n')
    def execute_native():
        if feedback_mode:
            from feedback import run_aider
            return run_aider(coder, a.files, a.output, feedback_mode, task)
        return coder.run(with_message=task)
    result, mode_result = run_mode(coder, a.files, a.output, mode_config, execute_native)
    (a.output/'native-result.json').write_text(json.dumps({'native_result':result},default=str,indent=2)+'\n')
    if mode_result['reflection_limit_reached'] or (feedback_mode and result['unresolved_native_feedback']):
        sys.exit(3)

elif a.repository == "R15" and a.mini_json:
    from mini_harness import run_owned
    result = run_owned(a, task, json.loads(a.mini_json))
    sys.exit(0 if result.get("exit_status") == "Submitted" else 3)

elif a.repository == "R15":
    import litellm
    import yaml
    from minisweagent.agents.default import DefaultAgent
    from minisweagent.environments.local import LocalEnvironment
    from minisweagent.models.litellm_textbased_model import LitellmTextbasedModel
    from minisweagent.config import builtin_config_dir
    config_bytes=(builtin_config_dir/'mini_textbased.yaml').read_bytes()
    (a.output/'native-prompt-config.yaml').write_bytes(config_bytes)
    config=yaml.safe_load(config_bytes)
    litellm.OllamaChatConfig(num_ctx=8192,seed=0)
    model = LitellmTextbasedModel(model_name="ollama_chat/"+a.model_name, cost_tracking="ignore_errors",
        observation_template=config['model']['observation_template'],format_error_template=config['model']['format_error_template'],
        model_kwargs={"api_base": model_base, "api_key": "local-no-secret",
                      "temperature": 0, "reasoning_effort":"none", "max_tokens": 1600, "timeout": 150, "num_retries": 0})
    env = LocalEnvironment(cwd=str(a.workspace), timeout=25)
    edit_instructions = ('The workspace is an isolated copy. Change only the selected source files. Sibling temporary files for ordinary editors are supported; remove temporary files before finishing. Git metadata and captured context are read-only.'
        if os.environ.get('THRULINE_NATIVE_NAMESPACE') == 'linux' else
        'Only the selected source files are writable. For edits use Python pathlib.Path.write_text, which writes directly; sed -i and atomic replacement create prohibited temporary files.')
    agent = DefaultAgent(model, env, step_limit=a.iterations, cost_limit=0, wall_time_limit_seconds=480,
        output_path=a.output / "trajectory.json", max_consecutive_format_errors=3,
        system_template=config['agent']['system_template']+'\n'+edit_instructions+' Run tests using python -c or python heredoc without creating test files. After a command fails, change your approach instead of repeating it.',
        instance_template=config['agent']['instance_template']+'\nThe actual working directory is {{cwd}}. Honor the user task file edit scope. Run tests through python -c without creating or changing test files.')
    result = agent.run(task)
    (a.output / "native-result.json").write_text(json.dumps(result, indent=2))
    sys.exit(0 if result.get("exit_status") == "Submitted" else 3)

elif a.repository == "R13":
    from openhands_native import run
    sys.exit(run(a, task))

else:
    import litellm
    from pr_agent.config_loader import get_settings
    from pr_agent.agent.pr_agent import PRAgent
    settings = {
        "config.git_provider": "local", "config.model": "openai/"+a.model_name,
        "config.fallback_models": [], "config.custom_model_max_tokens": 8192,
        "config.max_model_tokens": 8192, "config.max_output_tokens": 1800,
        "config.model_token_limits": {"openai/"+a.model_name: 8192},
        "config.publish_output": True, "config.cli_mode": True,
        "config.retry_same_model_on_timeout": False, "config.use_repo_settings_file": False,
        "config.extra_config_url": None, "config.ai_timeout": 150,
        "openai.api_base": model_base+"/v1", "openai.key": "local-no-secret",
        "litellm.enable_callbacks": False, "otel.is_enabled": False,
        "local.review_path": str(a.output / "review.md"),
        "local.description_path": str(a.output / "description.md"),
        "local.improve_path": str(a.output / "improve.md"),
        "pr_reviewer.extra_instructions": task, "pr_reviewer.require_tests_review": False,
        "pr_reviewer.require_score_review": False, "pr_reviewer.require_can_be_split_review": False,
        "pr_code_suggestions.extra_instructions": task,
    }
    if a.pr_agent_json:
        from pr_agent_options import configure_native
        configure_native(json.loads(a.pr_agent_json), a.workspace, a.output, a.command, settings)
    for key, value in settings.items():
        get_settings().set(key, value)
    # Dynaconf merges a dotted list assignment with the image's default
    # configuration. Replace the effective CONFIG mapping so the accepted
    # one-model route cannot retain an upstream fallback after a timeout.
    effective_config = dict(get_settings().config)
    effective_config.update({
        "model": "openai/" + a.model_name,
        "fallback_models": [],
        "retry_same_model_on_timeout": False,
        "use_repo_settings_file": False,
        "extra_config_url": None,
    })
    get_settings().set("config", effective_config, merge=False)
    effective = {
        "model": get_settings().config.model,
        "fallback_models": list(get_settings().config.fallback_models),
        "retry_same_model_on_timeout": get_settings().config.retry_same_model_on_timeout,
        "use_repo_settings_file": get_settings().config.use_repo_settings_file,
        "extra_config_url": get_settings().get("CONFIG.EXTRA_CONFIG_URL", None),
    }
    expected_model = "openai/" + a.model_name
    if (effective["model"] != expected_model or effective["fallback_models"] != []
            or effective["retry_same_model_on_timeout"] is not False
            or effective["use_repo_settings_file"] is not False
            or effective["extra_config_url"] is not None):
        raise SystemExit("native PR-Agent effective model route did not match the accepted single-model plan")
    (a.output / "native-settings.json").write_text(json.dumps({"requested": settings, "effective": effective}, indent=2))
    class NativeCallLog(litellm.integrations.custom_logger.CustomLogger):
        def log_pre_api_call(self, model, messages, kwargs):
            with (a.output / "model-inputs.jsonl").open("a") as f:
                f.write(json.dumps({"model": model, "messages": messages}, default=str) + "\n")
        async def async_log_success_event(self, kwargs, response_obj, start_time, end_time):
            with (a.output / "model-responses.jsonl").open("a") as f:
                f.write(response_obj.model_dump_json() + "\n")
    litellm.callbacks = [NativeCallLog()]
    async def run():
        result = await PRAgent().handle_request("local_diff" if a.pr_agent_json else "main", [a.command])
        await asyncio.sleep(0.5)
        return result
    result = asyncio.run(run())
    effective_after = {
        "model": get_settings().config.model,
        "fallback_models": list(get_settings().config.fallback_models),
        "retry_same_model_on_timeout": get_settings().config.retry_same_model_on_timeout,
        "use_repo_settings_file": get_settings().config.use_repo_settings_file,
        "extra_config_url": get_settings().get("CONFIG.EXTRA_CONFIG_URL", None),
    }
    if effective_after != effective:
        raise SystemExit("native PR-Agent settings changed during the accepted single-model request")
    (a.output / "native-settings.json").write_text(
        json.dumps({"requested": settings, "effective": effective, "effective_after": effective_after}, indent=2)
    )
    (a.output / "native-result.json").write_text(json.dumps({"result": result}, default=str, indent=2))
    if a.pr_agent_json:
        response = a.output / {'review': 'review.md', 'improve': 'improve.md', 'describe': 'description.md'}[a.command]
        structured = a.output / 'structured-review.json'
        if not response.is_file() or not response.read_text().strip() or (a.command == 'review' and not structured.is_file()):
            raise SystemExit('Native plain-diff did not retain the requested complete review output')
