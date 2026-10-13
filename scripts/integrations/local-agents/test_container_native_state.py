"""T-4-1 fix 1 (R04, Q11): a stop the product enforced on a native container
run is the state that run ends in, never `transport_failed`.

Docker-free: `run_state` is pure, and the second class reads `run_native`'s
source so an inline override of the decided state cannot come back unseen.
Run: python3 -B scripts/integrations/local-agents/test_container_native_state.py
(`cargo test` runs it too, from crates/cli/src/integration_cmd/review.rs).
"""
import ast
import importlib.util
import unittest
from pathlib import Path

FOLDER = Path(__file__).parent
spec = importlib.util.spec_from_file_location('owned_container_native', FOLDER / 'container_native.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ProductStopPrecedence(unittest.TestCase):
    def test_the_product_deadline_wins_over_every_transport_failure(self):
        for alive, errors in [(True, []), (False, ['model request timed out']), (True, ['x', 'y'])]:
            self.assertEqual(module.run_state('timed_out', alive, errors), 'timed_out',
                             f'deadline relabelled with broker_alive={alive} errors={errors}')

    def test_the_product_output_bound_wins_over_every_transport_failure(self):
        for alive, errors in [(True, []), (False, ['x']), (True, ['x'])]:
            self.assertEqual(module.run_state('output_limit', alive, errors), 'output_limit')

    def test_without_a_product_stop_a_transport_failure_is_still_named(self):
        self.assertEqual(module.run_state(None, True, []), 'transport_failed')
        self.assertEqual(module.run_state(None, False, ['request outside registered local model API']), 'transport_failed')

    def test_a_clean_run_exited(self):
        self.assertEqual(module.run_state(None, False, []), 'exited')

    def test_an_unknown_stop_is_refused_not_passed_through(self):
        for stop in ['transport_failed', 'exited', 'cancelled', '']:
            with self.assertRaises(ValueError):
                module.run_state(stop, False, [])


class RunNativeDecidesStateInOnePlace(unittest.TestCase):
    def runner(self):
        tree = ast.parse((FOLDER / 'container_native.py').read_text())
        return next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == 'run_native')

    def assigned(self, name):
        return [n.value for n in ast.walk(self.runner()) if isinstance(n, ast.Assign)
                and any(isinstance(t, ast.Name) and t.id == name for t in n.targets)]

    def test_state_is_assigned_only_from_run_state(self):
        values = self.assigned('state')
        self.assertEqual(len(values), 1, 'run_native must decide its state once')
        call = values[0]
        self.assertTrue(isinstance(call, ast.Call) and isinstance(call.func, ast.Name) and call.func.id == 'run_state',
                        'run_native must take its state from run_state()')

    def test_every_product_stop_it_records_is_one_run_state_knows(self):
        stops = [v.value for v in self.assigned('stop')]
        self.assertEqual(sorted(s for s in stops if s is not None), sorted(module.PRODUCT_STOPS))
        self.assertIn(None, stops)


if __name__ == '__main__':
    unittest.main()
