import copy,importlib.util,unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('owned_mini_harness',Path(__file__).with_name('mini_harness.py'));module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
class MiniHarnessSelectionTests(unittest.TestCase):
 def good(self):return {'schema':'thruline.mini-harness-options.v1','mode':'owned-swebench-task','environment':'local-in-linux','instance_id':'owned_case-01','command_timeout':25}
 def test_declared_mode_leaves_caller_unchanged(self):
  value=self.good();before=copy.deepcopy(value);result=module.validate(value);self.assertEqual(value,before);self.assertEqual(result,value);self.assertIsNot(result,value)
 def test_paths_and_shell_spellings_cannot_select_instance_roots(self):
  for name in ['../x','/tmp/x','a/b','x; echo y','','x'*81]:
   value=self.good();value['instance_id']=name
   with self.assertRaises(ValueError):module.validate(value)
 def test_environment_or_privilege_switch_is_not_accepted(self):
  for name in ['docker','singularity','bubblewrap','swerex_modal','contree']:
   value=self.good();value['environment']=name
   with self.assertRaises(ValueError):module.validate(value)
  value=self.good();value['run_args']=['--privileged']
  with self.assertRaises(ValueError):module.validate(value)
 def test_timeout_types_and_bounds_remain_limited(self):
  for timeout in [0,26,-1,True,1.0,'1']:
   value=self.good();value['command_timeout']=timeout
   with self.assertRaises(ValueError):module.validate(value)
 def test_scores_and_arbitrary_configuration_are_rejected(self):
  for key,entry in [('official_score',100),('model_class','other.Model'),('startup_command','anything')]:
   value=self.good();value[key]=entry
   with self.assertRaises(ValueError):module.validate(value)


class PlumbingTests(unittest.TestCase):
 def test_declared_option_reaches_actual_native_engine(self):
  import ast
  folder=Path(__file__).parent
  native=ast.parse((folder/'container_native.py').read_text())
  runner=next(n for n in native.body if isinstance(n,ast.FunctionDef) and n.name=='run_native')
  self.assertEqual(runner.args.args[-1].arg,'mini_options')
  self.assertIsNone(runner.args.defaults[-1].value)
  invoke=ast.parse((folder/'invoke.py').read_text())
  call=next(n for n in ast.walk(invoke) if isinstance(n,ast.Call) and isinstance(n.func,ast.Name) and n.func.id=='run_native')
  self.assertEqual(call.args[-1].id,'mini_options')
  self.assertIn("['--mini-json', json.dumps(mini_options",(folder/'container_native.py').read_text())

if __name__=="__main__": unittest.main()
