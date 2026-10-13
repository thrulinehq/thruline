# The Ruby honest/cheat families

Real Ruby, in the two dialects a Ruby suite is actually written in: RSpec
(`*_spec.rb`) and minitest (`*_test.rb`). Every head file is its base file with
**exactly one edit applied**, so a finding `tests/families_ruby.rs` sees can only
have come from that edit — an edit that also renamed a variable would make every
assertion in the test file a guess about which change produced the finding.

Two bases, because two of the cases cannot be written in RSpec at all:
`begin` / `rescue` is the shape a structural swallow detector has to tell apart
from an honest negative test, and it has no RSpec spelling — `expect { }.to
raise_error` is a matcher, not a handler.

These are **hand-written source fixtures, not captures**. That distinction is
the one `tests/fixtures/runners/README.md` insists on and it cuts the other way
here: runner fixtures must be bytes a real runner emitted, because the thing
under test is a parser of that runner's output. What is under test here is a
tree-sitter walk over Ruby source, so the fixture is source, and the only
provenance that matters is that it parses as Ruby and reads as something a
person would write. Both halves were checked: `ruby -c` accepts every file in
this directory, and the test that consumes them fails loudly if the parse yields
no tests.

The run counts that go with each pair — how many examples, how many skipped, how
many pending — live in `tests/families_ruby.rs` beside the case, because two of
§6.7's severities are decided by the run and not by the source.
