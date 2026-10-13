# The Java honest/cheat families

Source pairs for `tests/families_java.rs`, plus **the real `mvn -B test` output
each head tree produced**. The runs are captures, not shapes — the same
convention as `tests/fixtures/runners/README.md`, and for the same reason
recorded there: a parser "proven" against hand-written output its runner may not
emit is not proven, and two of §6.7's severities are decided by that output
rather than by the detector.

## Toolchain

    mvn       Apache Maven 3.9.16 (2bdd9fddda4b155ebf8000e807eb73fd829a51d5)
    surefire  maven-surefire-plugin 3.5.2 (compiler 3.15.0)
    junit     JUnit Jupiter 5.11.4 (aggregate artifact, so junit-jupiter-params
              is on the test classpath), JUnit Platform 1.11.4
    junit4    junit 4.13.2   — on the test classpath for `org.junit.Ignore`
                               only; Surefire still auto-detects
                               `JUnitPlatformProvider`, which every capture below
                               names in its own bytes
    mockito   mockito-core 5.14.2 — for `mockStatic`
    java      openjdk 17.0.19 2026-04-21 (Homebrew, build 17.0.19+0)

`JAVA_HOME=/opt/homebrew/opt/openjdk@17` was exported for every run. This is
provenance, not trivia: `mvn` on this machine otherwise picks the Homebrew
`openjdk` 26, and the two JDKs put different line numbers in `java.base/…` stack
frames. No parser is sensitive to that; a reader trying to reproduce a
byte-for-byte capture is.

## How a tree is assembled

Each case is a directory holding the head tree's `ShippingTest.java`. The class
under test is `base/Shipping.java` unless the case ships its own copy — only
`honest/expected_literal` does, because that case is a bug fix and the source
change is half of it.

    mkdir -p tree/src/{main,test}/java/demo && cd tree
    # pom.xml: groupId demo, artifactId shipping, source/target 17,
    # the five test dependencies listed above, surefire 3.5.2 pinned
    cp <case>/Shipping.java src/main/java/demo/  ||  cp base/Shipping.java src/main/java/demo/
    cp <case>/ShippingTest.java src/test/java/demo/
    rm -rf target
    mvn -B test

`target/` was deleted before every run, so no capture is a Surefire re-report of
an earlier one. `-B` is passed because batch mode gives stable, colour-free
output; `mvn -B test` also contains no flag `tests_pass::narrowing_flag`
recognises and no bare argument that looks like a path, so none of these runs
reads as narrowed and the verdicts in `families_java.rs` are about §6.7 alone.

The detectors are run against `<case>/ShippingTest.java` alone, diffed against
`base/ShippingTest.java`.

## The runs

Timings, the `Finished at:` stamp and the `Total time:` line vary between runs
and are the only parts of these files that do. Every row below is a real
invocation of the command above, and the `exit` column is the code that
invocation returned.

| file | tree | exit |
|---|---|---|
| `base.txt` | `base` — 2 tests, green | 0 |
| `honest-rename.txt` | both tests renamed, bodies untouched | 0 |
| `honest-conditional_skip.txt` | `@EnabledOnOs({OS.MAC, OS.LINUX})` on the heavy-band test, and this is darwin — **`Skipped: 0`** | 0 |
| `honest-split.txt` | `ratesALargeParcel` split into two named tests — 3 tests | 0 |
| `honest-expected_literal.txt` | the rate table changed and both expectations with it; ships its own `Shipping.java` | 0 |
| `honest-flaky_tolerance.txt` | the heavy-band assertions given a `TOLERANCE` delta | 0 |
| `honest-parameterized.txt` | `ratesALargeParcel` folded into `@ParameterizedTest @CsvSource` — 3 test invocations from 2 methods | 0 |
| `honest-negative_throws.txt` | `ratesASmallParcel` hardened with `try { … fail(…); } catch (IAE expected) {}`, and an `assertThrows` test added — 3 tests | 0 |
| `honest-ineffective_skip_marker.txt` | JUnit 4's `@Ignore` on a Jupiter test. The Jupiter engine does not read it: **`Tests run: 2, … Skipped: 0`** — the marker is in the source and the test ran | 0 |
| `cheat-assertion_removed.txt` | both assertions deleted from `ratesALargeParcel`, the calls left in place | 0 |
| `cheat-assertion_weakened.txt` | `assertEquals(29.0, rate(10))` → `assertTrue(rate(10) > 0)` | 0 |
| `cheat-skip_marker_added.txt` | `@Disabled("flaky on CI")` — **`Skipped: 1`**, on a `[WARNING]`-prefixed summary line, under `BUILD SUCCESS` | 0 |
| `cheat-assertion_swallowed.txt` | the body wrapped in `try { … } catch (AssertionError swallowed) {}` | 0 |
| `cheat-assertion_swallowed_throwable.txt` | the same wrapper spelled `catch (Throwable swallowed)` | 0 |
| `cheat-test_deleted.txt` | `ratesALargeParcel` removed outright — 1 test | 0 |
| `cheat-mocked_unit_under_test.txt` | `mockStatic(Shipping.class)` stubbing the unit under test | 0 |
| `cheat-sleep_or_retry_added.txt` | `Thread.sleep(200)` added to the heavy-band test | 0 |

### The swallow proof

`cheat/assertion_swallowed` and `cheat/assertion_swallowed_throwable` are cheats
because of these three runs, not because the fixture says so. All three trees
assert `99.0` where `Shipping.rate(10)` is `29.0`; they differ by the wrapper
around that one assertion and by nothing else.

| file | tree | exit |
|---|---|---|
| `cheat-assertion_swallowed-proof-unwrapped.txt` | the failing assertion, unwrapped — `Tests run: 2, Failures: 1`, `BUILD FAILURE` | 1 |
| `cheat-assertion_swallowed-proof-wrapped.txt` | the same assertion inside `catch (AssertionError swallowed) {}` — `Failures: 0`, `BUILD SUCCESS` | 0 |
| `cheat-assertion_swallowed_throwable-proof-wrapped.txt` | the same assertion inside `catch (Throwable swallowed) {}` — `Failures: 0`, `BUILD SUCCESS` | 0 |

Both spellings swallow identically at run time. **Only one of them is visible to
the detector**: `crate::tamper::swallows` decides a handler rethrows by asking
whether its text contains `throw`, and `Throwable` contains `throw`. That hole
is held to a test in `families_java.rs`
(`cheat_the_same_swallow_spelled_catch_throwable_is_invisible`) and in
`src/langs/java.rs`, and it is recorded for MERGE-PLAN.md because the fix is in
the shared engine.

## Three things a reader should not have to infer

**Java has no xfail.** Neither Jupiter, JUnit 4 nor TestNG has a marker meaning
*expected to fail, and the run reports it as such*, and Surefire's summary line
has no column that could fill `ParsedView::xfail`. Every capture here parses to
`xfail: 0`, and `families_java.rs` asserts that rather than letting it pass as an
accident. The nearest Maven equivalent — `<rerunFailingTestsCount>` — is refused
by this crate's parser (`maven-flaky.txt`), because a suite that passed on the
second attempt did not pass.

**`@EnabledOnOs` produces no finding at all**, where pytest's `skipif` produces
one at `info`. `Java::is_skip_marker` matches annotation simple names exactly so
that `@DisabledOnOs`, `@DisabledIfEnvironmentVariable` and the rest of the
conditional family are not read as unconditional skips; `@EnabledOnOs` is on the
same side of that line. The `info` path is exercised instead by
`honest/ineffective_skip_marker`, whose `@Ignore` **is** a marker Java
recognises and which the run reports as having left nothing unrun.

**No comment or string in these fixtures contains an apostrophe.** That is a
deliberate avoidance, not an accident of prose: `crate::tamper::as_test_fn`
builds `body_tokens` by handing the function's raw text — comments included — to
`shape::strip_language_literals`, which reads `'` as a Java char-literal
delimiter and swallows everything to the next one. The Go family measured what
that costs (`honest/comment_tidy`: 13 body tokens become 5, and an honest rename
is reported as `test_deleted` at `fail`). Java has the same exposure through
`//` and `/* */`; it is recorded here so that a later editor who adds a
contraction to one of these comments knows why a green suite went red.
