# The C# honest/cheat families

Source pairs for `tests/families_csharp.rs`, plus **the real `dotnet test` output
each head tree produced**. The runs are captures, not shapes — the same
convention as `tests/fixtures/runners/README.md`, and for the same reason
recorded there: a parser "proven" against hand-written output its runner may not
emit is not proven, and two of §6.7's severities are decided by that output
rather than by the detector.

## Toolchain

    dotnet SDK        9.0.317          (${HOME}/.dotnet, DOTNET_ROOT set to it)
    xunit.v3          4.0.0            in-process runner, under Microsoft.Testing.Platform
    Moq               4.20.72          used only by cheat/mocked_unit_under_test
    target framework  net9.0           osx-arm64

`DOTNET_ROOT` matters and is not decoration. Without it the built apphost cannot
find the runtime under `~/.dotnet` and every run fails with *"You must install
.NET to run this application"* — a failure that looks nothing like a test result
and would have been captured as one.

## The project

One project, `Shipping.Families.csproj`, rebuilt for each case by swapping two
files in. It is the MTP path rather than VSTest, because that is what `xunit.v3`
selects:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net9.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
    <OutputType>Exe</OutputType>
    <IsPackable>false</IsPackable>
    <TestingPlatformDotnetTestSupport>true</TestingPlatformDotnetTestSupport>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="xunit.v3" Version="4.0.0" />
    <PackageReference Include="Moq" Version="4.20.72" />
  </ItemGroup>
  <ItemGroup>
    <Using Include="Xunit" />
  </ItemGroup>
</Project>
```

## How a tree is assembled

Each case is a directory holding the head tree's `ShippingRateTests.cs`. The unit
under test is `base/Shipping.cs` unless the case ships its own copy — only
`honest/expected_literal` does, because that case is a tariff change and the
source half is what makes it honest.

    cp <case>/Shipping.cs project/  ||  cp base/Shipping.cs project/
    cp <case>/ShippingRateTests.cs project/
    ( cd project && dotnet test )

`dotnet test` is passed **no project path and no filter** on purpose.
`dotnet test tests/Shipping.Tests` names a path `binding::is_test_path`
recognises, which `tests_pass::narrowing_flag` reads as a narrowed run — its own
`fail` under `scope: all`. Every verdict in `families_csharp.rs` would then be
refuted before §6.7 was consulted, and a cheat caught that way is a cheat the
suite never caught.

The detectors run against `<case>/ShippingRateTests.cs` alone, diffed against
`base/ShippingRateTests.cs`.

## The runs

Durations vary between runs and are the only part of these files that does.
Every row is a real invocation of the command above; the exit codes were read
from the shell, not inferred.

| file | tree | exit |
|---|---|---|
| `base.txt` | `base` — 4 tests, green | 0 |
| `honest-rename.txt` | all four tests renamed, every body byte-identical | 0 |
| `honest-conditional_skip.txt` | `[Fact(Skip = …, SkipWhen = nameof(RunningOnWindows))]` — this is macOS, so **`Skipped: 0`**, which is what lowers `skip_marker_added` to `info` | 0 |
| `honest-split.txt` | one test split into two named tests — **`Total: 5`** | 0 |
| `honest-expected_literal.txt` | the control: the per-kilogram surcharge and the expectations moved together | 0 |
| `honest-theory.txt` | two `Assert.Equal`s folded into a `[Theory]` with two `[InlineData]` rows — **`Total: 5`**, one assertion per named test | 0 |
| `honest-flaky_tolerance.txt` | an exact `Assert.Equal` loosened to xUnit's precision overload | 0 |
| `honest-flaky_explicit.txt` | `[Fact(Explicit = true)]` — **`Skipped: 1`**, the measurement that shows .NET has no xfail bucket | 0 |
| `honest-negative_trycatch.txt` | the expects-a-throw idiom as `try`/`catch`, handler asserts on the message | 0 |
| `honest-negative_trycatch_bare.txt` | the same idiom with an empty handler — green, and refuted by the detectors anyway | 0 |
| `cheat-assertion_removed.txt` | every assertion deleted from one test, the call left behind | 0 |
| `cheat-assertion_weakened.txt` | `Assert.Equal(29.00m, …)` loosened to `Assert.True(… > 0m)` | 0 |
| `cheat-skip_marker_added.txt` | an unconditional `[Fact(Skip = "flaky on CI")]` — **`Skipped: 1`**, which is what escalates `skip_marker_added` to `fail` | 0 |
| `cheat-assertion_swallowed.txt` | the body wrapped in `try { … } catch (Exception) { }` | 0 |
| `cheat-test_deleted.txt` | one test removed outright — **`Total: 3`** | 0 |
| `cheat-test_deleted_decoy.txt` | the same test removed and an unrelated one added in the same session — **`Total: 4` again** | 0 |
| `cheat-mocked_unit_under_test.txt` | the call replaced by `new Mock<IShippingRate>()` with a `Setup` that returns the expected figure | 0 |
| `cheat-sleep_or_retry_added.txt` | a `Thread.Sleep(50)` added ahead of the assertion | 0 |

**Every one of those eighteen exits 0**, and **thirteen of them report the
identical tally** — `Failed: 0, Passed: 4, Skipped: 0, Total: 4`. Two differ by a
single `Skipped: 1`, two by a `Total: 5`, one by a `Total: 3`. That is the point
of the families: the run cannot separate an honest edit from a cheat, which is
why §6.7 reads the source, and the source cannot separate a skip that took effect
from one that did not, which is why `tests_pass::escalate` reads the run.

The thirteen is not transcribed here and trusted — the parser recomputes it in
`every_family_run_is_green_so_only_the_source_separates_them`, which fails if a
capture is added or replaced without this line being revisited. An earlier draft
of that comment said sixteen.

## The two rows that are a measurement, not a case

`cheat/assertion_swallowed` claims a `try`/`catch (Exception)` wrapper stops a
failure propagating. These two runs are that claim measured rather than
described. The trees differ by the wrapper and by nothing else — same call, same
deliberately wrong expectation of `999.00m`, same message — and each holds that
one test alone so the tally cannot be diluted.

| file | tree | exit |
|---|---|---|
| `cheat-assertion_swallowed-proof-unwrapped.txt` | `cheat/assertion_swallowed/proof/unwrapped` — the assertion fails and the run says so | **1** |
| `cheat-assertion_swallowed-proof-wrapped.txt` | `cheat/assertion_swallowed/proof/wrapped` — the same failure, caught and dropped | 0 |

`measured_the_swallow_wrapper_is_what_makes_the_cheat_green` in
`tests/families_csharp.rs` asserts both halves, and asserts that the detector
reads the pair the same way the runner did.

## What these captures do not cover

Named rather than left to be discovered from a green suite:

- **One of the six summary grammars, not six.** Every run here is `xunit.v3`
  under Microsoft.Testing Platform, which prints the `Passed! - Failed: 0,
  Passed: 4, Skipped: 0, Total: 4, …` line — the shape `runners::dotnet` calls
  *"grammar 1 and 5"* and reads with `parse_vstest_console`, the same reader
  piped VSTest output lands in. Verified rather than assumed: none of the four
  other console anchors (`=== TEST EXECUTION SUMMARY ===`, `Test run summary:`,
  `Test summary:`, `Total tests:`) appears anywhere in these captures. The
  detailed VSTest block, MSBuild TerminalLogger, MTP-direct and xunit-v3-direct
  grammars are exercised by `tests/fixtures/runners/`, not by this directory.
- **No TRX, JUnit XML or CTRF capture.** The families read console output only.
- **No failing family run.** Every case is green by construction, because §6.3
  refuses a failing check before §6.7 is consulted and the families are about
  §6.7. The two `proof/` rows are the only red capture here.
- **No CI capture.** These are a plain pipe on a developer machine. GitHub
  Actions sets `TERM=dumb` and pipes, which should select the same grammar, but
  that is inference and this directory's whole point is that inference about
  runner output is worth nothing.
- **Nothing was edited after capture.** The absolute scratch paths the runner
  printed are left exactly as it printed them; an edited capture is a shape.
