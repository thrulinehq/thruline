"""Collector-owned unittest adapter. Test output is captured separately from the protocol.
This runs trusted local code; it is not a hostile-code isolation mechanism.
"""
import contextlib
import json
import os
import sys
import unittest

plan = json.loads(sys.argv[1])
sys.path.insert(0, os.getcwd())
rows = {}

class Observed(unittest.TestResult):
    def startTest(self, test):
        super().startTest(test)
        rows[test.id()] = {"id": test.id(), "state": "running"}
    def addSuccess(self, test):
        super().addSuccess(test)
        rows[test.id()] = {"id": test.id(), "state": "pass"}
    def addFailure(self, test, err):
        super().addFailure(test, err)
        rows[test.id()] = {"id": test.id(), "state": "fail", "detail": str(err[1])[:1000]}
    def addError(self, test, err):
        super().addError(test, err)
        rows[test.id()] = {"id": test.id(), "state": "error", "detail": str(err[1])[:1000]}
    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        rows[test.id()] = {"id": test.id(), "state": "skip", "detail": reason[:1000]}
    def addExpectedFailure(self, test, err):
        super().addExpectedFailure(test, err)
        rows[test.id()] = {"id": test.id(), "state": "expected_failure"}
    def addUnexpectedSuccess(self, test):
        super().addUnexpectedSuccess(test)
        rows[test.id()] = {"id": test.id(), "state": "unexpected_success"}
    def addSubTest(self, test, subtest, err):
        super().addSubTest(test, subtest, err)
        if err is not None:
            rows[test.id()] = {"id": test.id(), "state": "fail", "detail": str(err[1])[:1000]}

def flatten(suite):
    for test in suite:
        if isinstance(test, unittest.TestSuite):
            yield from flatten(test)
        else:
            yield test.id()

with contextlib.redirect_stdout(sys.stderr):
    loader = unittest.TestLoader()
    suite = loader.discover(plan["directory"], pattern=plan["pattern"])
    discovered = list(flatten(suite))
    if len(discovered) > 256:
        raise RuntimeError("discovery exceeds 256 tests")
    result = Observed()
    suite.run(result)
payload = {"protocol": "thruline-unittest/2", "directory": plan["directory"], "pattern": plan["pattern"],
           "discovered": discovered, "tests_run": result.testsRun, "rows": list(rows.values()), "loader_errors": loader.errors}
sys.stdout.write(json.dumps(payload))
sys.exit(0 if result.wasSuccessful() else 1)
