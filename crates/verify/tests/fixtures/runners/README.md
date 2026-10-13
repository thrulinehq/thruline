# Runner output fixtures

**Captured from real runs on this machine**, not written from documentation.
Provenance, so a later reader can tell a capture from a shape:

| file | produced by |
|---|---|
| `mocha-mixed.txt` | `npx mocha` — mocha 11.x, node v22.19.0 |
| `mocha-no-test-files.txt` | `npx mocha "empty/*.js"` |
| `mocha-collection-error.txt` | `npx mocha` on a file with a bad `require` |
| `jest-mixed.txt` | `npx jest --ci` — jest 30.x, node v22.19.0 |
| `jest-no-tests.txt` | `npx jest --passWithNoTests` on an empty directory |
| `jest-collection-error.txt` | `npx jest --ci` on a file with a bad `require` |
| `pytest-junit.xml` | `pytest --junit-xml=` — pytest 9.0.1 |

The reason this directory exists is `pytest -q`: that parser was "proven"
against a hand-written fixture carrying a `=====` banner pytest does not emit,
and the first real captured run came back `unverifiable`. A parser proven
against output its runner may not produce is not proven.

**`go test` has no capture here** — there is no Go toolchain on this machine, so
its fixtures remain written from the documented formats and are marked as such
in `remaining_parsers.rs` and in `REPORT.md`. A gap that is named is fine.
