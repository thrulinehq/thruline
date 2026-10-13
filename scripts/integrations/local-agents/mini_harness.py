"""Owned SWE-bench-format task through the pinned upstream native harness.

This is compatibility execution, not an official benchmark evaluation.
"""
import json, os, re, shutil
from pathlib import Path

def run_owned(a, task, options):
 options=validate(options)
 import litellm,yaml
 from minisweagent.config import builtin_config_dir
 from minisweagent.run.benchmarks.swebench import filter_instances,process_instance
 from minisweagent.run.benchmarks.utils.batch_progress import RunBatchProgressManager
 if os.environ.get('THRULINE_NATIVE_NAMESPACE')!='linux':raise ValueError('Owned batch requires the product Linux namespace')
 config_bytes=(builtin_config_dir/'mini_textbased.yaml').read_bytes();(a.output/'native-prompt-config.yaml').write_bytes(config_bytes);native=yaml.safe_load(config_bytes)
 litellm.OllamaChatConfig(num_ctx=8192,seed=0)
 instance={'instance_id':options['instance_id'],'problem_statement':task}
 selected=filter_instances([instance],filter_spec=re.escape(instance['instance_id'])+'$',slice_spec='0:1',shuffle=False)
 config={'environment':{'environment_class':'local','cwd':str(a.workspace),'timeout':options['command_timeout']},'model':{'model_class':'minisweagent.models.litellm_textbased_model.LitellmTextbasedModel','model_name':'ollama_chat/'+a.model_name,'cost_tracking':'ignore_errors','observation_template':native['model']['observation_template'],'format_error_template':native['model']['format_error_template'],'model_kwargs':{'api_base':'http://127.0.0.1:11435','api_key':'local-no-secret','temperature':0,'reasoning_effort':'none','max_tokens':1600,'timeout':150,'num_retries':0}},'agent':{'step_limit':a.iterations,'cost_limit':0,'wall_time_limit_seconds':420,'max_consecutive_format_errors':3,'system_template':native['agent']['system_template']+'\nThe workspace is an isolated copy. Change only selected source files. Captured context and Git metadata are read-only. Remove temporary files before finishing. Tests must use python -c or a heredoc without creating test files. After a failed command change your approach.','instance_template':native['agent']['instance_template']+'\nThe actual working directory is {{cwd}}. Do not modify acceptance or configuration files.'}}
 (a.output/'native-harness-config.json').write_text(json.dumps(config,indent=2)+'\n');(a.output/'owned-instance.json').write_text(json.dumps(instance,indent=2)+'\n')
 progress=RunBatchProgressManager(len(selected),a.output/'native-exit-statuses.yaml')
 for item in selected:process_instance(item,a.output,config,progress)
 trajectory=a.output/instance['instance_id']/(instance['instance_id']+'.traj.json')
 if not trajectory.is_file():raise ValueError('Native harness did not retain a trajectory')
 data=json.loads(trajectory.read_text());info=data['info'];shutil.copyfile(trajectory,a.output/'trajectory.json')
 result={'exit_status':info.get('exit_status'),'submission':info.get('submission')}
 (a.output/'native-result.json').write_text(json.dumps(result,indent=2)+'\n')
 summary={'schema':'thruline.mini-owned-harness.v1','native_runner':'minisweagent.run.benchmarks.swebench.process_instance','native_agent':'minisweagent.run.benchmarks.utils.common.ProgressTrackingAgent','environment_type':info.get('config',{}).get('environment_type'),'instance_ids':[v['instance_id'] for v in selected],'workers':1,'dataset_kind':'owned_compatibility_format','official_benchmark':False,'official_score':None,'native_predictions_file':'preds.json','native_trajectory_file':str(trajectory.relative_to(a.output)),'native_exit_status':info.get('exit_status'),'scope':'One owned task per disposable product namespace; upstream predictions are model submissions, not independent test results. No official SWE-bench/ProgramBench task or score is claimed.'}
 (a.output/'mini-harness.json').write_text(json.dumps(summary,indent=2)+'\n');return result

def validate(value):
 if not isinstance(value,dict) or set(value)!={'schema','mode','environment','instance_id','command_timeout'}:
  raise ValueError('mini-SWE harness requires exactly the declared configuration fields')
 if value['schema']!='thruline.mini-harness-options.v1' or value['mode']!='owned-swebench-task' or value['environment']!='local-in-linux':
  raise ValueError('Only owned SWE-bench-format tasks in the product Linux LocalEnvironment are configured')
 if not isinstance(value['instance_id'],str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,79}',value['instance_id']):
  raise ValueError('Owned instance identifier must be a bounded path-free identifier')
 if type(value['command_timeout']) is not int or not 1<=value['command_timeout']<=25:
  raise ValueError('Native command timeout must be1–25 seconds')
 return dict(value)
