# ThruLine evidence-linked verification check catalog

**134 proposed checks across 15 families. Research cutoff: 8 September 2026.**

134 proposed check specifications, not 134 implemented scanners. Build adapters and validate each check on real projects before enabling gates.

Select applicable checks by accepted project contracts, changed inputs and risk. Do not run every check on every edit.

## How to use this catalog

Choose the project's accepted requirements and applicable checks first. Implement an adapter with a reliable oracle, then validate both fixtures and real-project behavior. A check specification is a contract for an implementation, not an executable scanner. Every result uses the common evidence contract below.

P0 means foundation or early high-consequence work; P1 means the next applicable domain pack; P2 means advanced or contextual work. These are sequencing recommendations, not a schedule or an automatic severity label. Severity belongs to the project-specific finding.

Cost: XS = metadata; S = static/index; M = targeted execution or review; L = integration, performance, soak, fuzz or deeper analysis. These classes are planning estimates, not measured runtimes.

Human-review checks remain advisory until an identified reviewer makes a scoped decision. No check automatically deletes code, rewrites accepted requirements, or authorizes an external action.

## Common evidence contract

Capture authenticity, oracle independence, artifact binding, freshness, coverage and parser fidelity are separate dimensions. A hash chain does not establish that the original claim was truthful. A local collector with the same privileges as the coding agent is not a strong adversarial boundary.

| Field group | Required content |
|---|---|
| identity | `check_id`, `check_version`, `instance_id`, `run_id`, `project_id`, `workspace_identity` |
| acceptance | `requirement_id`, `requirement_revision`, `criterion_id`, `accepted_policy_digest` |
| inputs | `artifact_manifest_digest`, `relevant_dirty_and_untracked_inputs`, `resolved_dependency_manifest_digest`, `input_completeness`, `dataset_digest` |
| execution | `collector_identity`, `capture_trust_boundary`, `run_start`, `run_end_or_open`, `executable_digest_or_version`, `argv`, `cwd`, `redacted_effective_environment_fingerprint`, `exit_code_or_signal` |
| oracle | `oracle_digest`, `oracle_authority`, `oracle_change_authorization`, `adapter_version`, `parser_fidelity` |
| result | `status`, `typed_facts`, `evidence_blob_digests_and_ranges`, `coverage`, `truncation`, `limitations`, `resource_metrics` |
| history | `invalidation_dependencies`, `superseded_result_ids`, `reason_rule_id`, `recorded_at`, `integrity_chain_reference` |

Use an explicit not_applicable reason for non-process or human-review results. Human decisions require reviewer identity, decision, scope and timestamp. Never fabricate executable fields for an imported assertion.

### Result states

| State | Meaning |
|---|---|
| `reported` | An actor asserted an outcome; the implementation criterion is not independently established. |
| `queued` | Work was accepted by the scheduler but has not started. |
| `running` | Execution began and no terminal observation has been captured. |
| `pass` | This accepted, applicable criterion is satisfied by admissible current evidence for the stated scope. |
| `fail` | Admissible evidence demonstrates a violation of this accepted criterion. |
| `unknown` | Evidence, interpretation, authority or coverage is insufficient to settle the criterion. |
| `stale` | A previously relevant result was invalidated by a relevant input or policy change. |
| `blocked` | The required verification could not run or finish; reason is explicit. |
| `not_applicable` | An explicit applicability rule excludes this criterion with a recorded reason. |
| `waived` | An authorized actor accepted an exception with scope/reason/expiry; this is not a pass. |

### Admission and invalidation

1. Determine applicability from accepted project policy; do not silently skip applicable checks.
2. Validate capture provenance, oracle authority, input binding, interpretation and required evidence coverage separately.
3. Preserve observation events; derive result state deterministically from frozen events and policies.
4. Only an admissible concrete counterexample supports fail; absence of proof yields unknown, not a bug accusation.
5. Only a completed admissible result satisfying the criterion supports pass; aggregate completion does not hide unknown or waived items.
6. Retain historical results; relevant changes append invalidations instead of overwriting history.

Results become stale after a relevant source, transitive dependency, requirement, oracle, adapter, dataset, environment or policy changes. Include relevant dirty and untracked inputs. If dependency completeness is unknown, invalidate conservatively. Keep historical results and the reason for invalidation.

## Family index

| Family | Checks | Focus |
|---|---:|---|
| REQ | 8 | Requirements and scope |
| EVD | 12 | Evidence integrity and authority |
| FUN | 8 | Functional behavior and contracts |
| TST | 10 | Test validity and independence |
| ARC | 8 | System architecture |
| ALG | 8 | Algorithms and data structures |
| DAT | 8 | Data integrity and persistence |
| SEC | 12 | Application and supply-chain security |
| PER | 10 | Performance and resource use |
| REL | 8 | Concurrency and resilience |
| UX | 8 | User experience and accessibility |
| DEL | 8 | Build, deployment and operations |
| MNT | 8 | Maintainability and unnecessary work |
| AGT | 10 | Agent workflow and control |
| AIP | 8 | Products that themselves use AI |

## REQ: Requirements and scope

Research context: [2], [15], [16]. These are proposed checks, not claims of shipped coverage.

### REQ-01 - Accepted requirement identity

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** core state integrity

**Applies / trigger:** Every proposed or revised task; on user instruction change

**Required evidence:** Versioned requirement text, accepting actor/event, scope and supersession links. Also require the common evidence contract.

**Pass:** Every active obligation has an attributable acceptance event and stable revision.

**Fail:** A requirement is presented as accepted although only an agent proposed it.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Ambiguous instructions stay pending clarification; do not invent acceptance

**Validation pair:** conforming - User accepts a criterion; violating - agent alone inserts a new mandatory criterion.

### REQ-02 - Measurable acceptance criteria

**Priority:** P0 · **Method:** human adjudicated review · **Cost:** XS · **Gate:** advisory until human decision

**Applies / trigger:** Material work before implementation or handoff

**Required evidence:** Requirement-to-criterion map with observable outcomes and reviewer decisions. Also require the common evidence contract.

**Pass:** A reviewer accepts criteria covering the intended outcome and constraints.

**Fail:** Reviewer identifies a material obligation with no usable acceptance criterion.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Automatic text similarity cannot prove complete interpretation

**Validation pair:** conforming - Save-and-reload criterion accepted; violating - criterion checks only that a button exists.

### REQ-03 - Completion coverage

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Task completion claim or handoff

**Required evidence:** Active requirement revisions, applicable check instances and current outcomes. Also require the common evidence contract.

**Pass:** Every mandatory criterion has a current admissible pass or an explicitly recorded waiver shown separately.

**Fail:** Completion is claimed while an applicable criterion is failed, missing or stale.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Completeness is bounded by accepted criteria; a waiver is never a pass

**Validation pair:** conforming - All required criteria pass; violating - authentication criterion omitted from summary.

### REQ-04 - Explicit constraints and versions

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Dependency, platform, API or configuration changes

**Required evidence:** Accepted constraints, resolved versions, environment manifest and artifact snapshot. Also require the common evidence contract.

**Pass:** Observed versions and settings satisfy every machine-checkable accepted constraint.

**Fail:** Resolved environment violates a specified version, platform or runtime restriction.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Natural-language compatibility requirements may require review

**Validation pair:** conforming - Requested version 5.2 is exercised; violating - tests run only against 4.0.0.

### REQ-05 - Scope expansion and unauthorized changes

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Diff grows beyond the accepted change scope

**Required evidence:** Accepted path/resource/action scope, diff and authorization receipts. Also require the common evidence contract.

**Pass:** Changed resources and performed actions lie inside the accepted scope.

**Fail:** An observed change or action crosses an explicit scope boundary without authorization.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Unexpected does not mean malicious; broad user intent can authorize routine edits

**Validation pair:** conforming - Feature diff stays in allowed modules; violating - unrelated billing configuration changes.

### REQ-06 - Dropped obligations versus unfinished work

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Session moves on or a handoff is requested

**Required evidence:** Accepted obligation, complete relevant event window, actual action links and later disposition. Also require the common evidence contract.

**Pass:** Each obligation is fulfilled, active, deferred, cancelled or explicitly unresolved with receipts.

**Fail:** A supported abandonment rule fires for a material obligation after the workflow moved on.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: No evidence is not proof of no work; incomplete capture returns unknown

**Validation pair:** conforming - Promise receives a linked edit and remains pending; violating - moved-on promise has no later relevant action.

### REQ-07 - Supersession and conflicting instructions

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** core state integrity

**Applies / trigger:** Requirements or constraints change

**Required evidence:** Instruction order, actors, scopes and explicit supersession edges. Also require the common evidence contract.

**Pass:** Current checks use the latest applicable accepted requirement while history remains intact.

**Fail:** A superseded requirement drives a current pass/fail or incompatible active constraints are hidden.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Ambiguous precedence needs an accepted resolution

**Validation pair:** conforming - User changes output format and checks follow it; violating - old format remains the only criterion.

### REQ-08 - Handoff fidelity

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Resume brief, progress summary or exported status

**Required evidence:** Frozen graph revision, requirement states and admissible evidence links. Also require the common evidence contract.

**Pass:** Every factual brief statement preserves its underlying status, scope and evidence reference.

**Fail:** An unknown, stale, reported or waived item becomes verified in the brief.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A concise brief may omit detail only with links and explicit outstanding coverage

**Validation pair:** conforming - Brief preserves a pending migration; violating - brief calls that migration completed.


## EVD: Evidence integrity and authority

Research context: [1], [4], [18], [22]. These are proposed checks, not claims of shipped coverage.

### EVD-01 - Raw record integrity

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Import, verification, raw file change or restore

**Required evidence:** Raw bytes, per-line hashes, full-file digest, sequence and length-prefixed chain encoding version. Also require the common evidence contract.

**Pass:** Recomputed hashes and sequence agree with the stored capture manifest.

**Fail:** A hash, count or chain link differs from the stored manifest.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: This establishes integrity relative to capture, not truthful origin

**Validation pair:** conforming - Unchanged malformed line verifies; violating - one byte in that malformed line changes.

### EVD-02 - Capture origin and observer authority

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** core state integrity

**Applies / trigger:** Every imported or captured result

**Required evidence:** Collector identity, trust boundary, transport, signature if applicable and source class. Also require the common evidence contract.

**Pass:** Result origin is authenticated to the level required by the specific check.

**Fail:** An imported or self-authored assertion is labeled independently captured evidence.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A local same-user collector is not independent of an equally privileged agent

**Validation pair:** conforming - Protected runner receipt accepted for its scope; violating - agent-authored log promoted to protected proof.

### EVD-03 - Collector health and missing intervals

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** core state integrity

**Applies / trigger:** Startup, reconnect, new session and heartbeat deadline

**Required evidence:** Startup canary, heartbeat, acknowledged hooks, sequence gaps and coverage intervals. Also require the common evidence contract.

**Pass:** Required capture channels pass a canary and show continuous coverage for the relevant interval.

**Fail:** A required channel is demonstrably inactive while coverage is displayed as complete.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A heartbeat timeout proves an observation gap, not that the task failed

**Validation pair:** conforming - Hook canary is recorded; violating - hook configured but never invoked.

### EVD-04 - Actual execution and process outcome

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** core state integrity

**Applies / trigger:** Command completion or build/test success claim

**Required evidence:** Collector-owned run ID, executable/argv, cwd, start/end, exit/signal and linked output. Also require the common evidence contract.

**Pass:** The required command actually completes with the contract's expected process outcome.

**Fail:** Observed exit/signal contradicts the claimed outcome, or execution never began despite a run claim.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Exit zero alone does not prove behavior; intentional failing probes need their own expectation

**Validation pair:** conforming - Expected build exits zero; violating - child process fails while a wrapper echoes success.

### EVD-05 - Exact suite, count and coverage scope

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Test-result claims and check aggregation

**Required evidence:** Structured test cases, suite/filter/shard identities, counts, skips and parser version. Also require the common evidence contract.

**Pass:** Claimed totals and scope match complete interpreted test results.

**Fail:** Observed results contradict stated counts/outcomes, or metadata proves a test specifically claimed for this run was excluded.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Narrow-suite evidence alone leaves an all-suites claim unknown; unknown formats or incomplete shards cannot establish a full-suite pass

**Validation pair:** conforming - 17 pass and 1 fail described accurately; violating - 18 of 18 claimed green.

### EVD-06 - Artifact and environment binding

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Any result admission, code edit or dependency/configuration change

**Required evidence:** Content-addressed source manifest including dirty and relevant untracked inputs, lockfiles, toolchain, configuration and dataset. Also require the common evidence contract.

**Pass:** Result references the exact inputs required by its check and matches the current applicability scope.

**Fail:** A result is attributed to a different artifact or relevant environment.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: HEAD alone omits dirty files; unknown build inputs prevent strong binding

**Validation pair:** conforming - Untracked generated source included; violating - same commit with different dirty code reuses a pass.

### EVD-07 - Freshness and dependency invalidation

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Any input or accepted-policy revision changes

**Required evidence:** Reverse dependency index, prior input digests, new digests and invalidation reasons. Also require the common evidence contract.

**Pass:** No relevant input changed since the admitted result, or a fresh run replaced it.

**Fail:** A known relevant change leaves an old result displayed as current.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: An incomplete dependency graph requires conservative invalidation, not optimistic reuse

**Validation pair:** conforming - Documentation-only edit retains isolated math result; violating - changed helper leaves dependent result green.

### EVD-08 - Temporal and causal scope

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Matching claims to earlier or later evidence

**Required evidence:** User-turn boundaries, run ordering, causal references and claim timestamps. Also require the common evidence contract.

**Pass:** Evidence supports the claim within its declared temporal scope without rewriting history.

**Fail:** Evidence from later work is used to claim an earlier unsupported completion was already verified.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Same-turn later evidence can settle a turn summary when that scope is explicit

**Validation pair:** conforming - Same-turn run supports summary; violating - next user turn's fix retroactively verifies earlier claim.

### EVD-09 - Semantic command and target matching

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Run matching across cwd, shell spelling, worktrees or retries

**Required evidence:** Parsed argv, shell structure, cwd, environment, target and filter identities. Also require the common evidence contract.

**Pass:** Equivalent commands resolve to the same relevant target and execution conditions.

**Fail:** Material flags, directories or targets differ despite claimed equivalence.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Never strip semantic flags or treat arbitrary shell text as a safe canonical form

**Validation pair:** conforming - cd package and explicit cwd agree; violating - omitted test filter makes two runs appear equivalent.

### EVD-10 - Evidence completeness and truncation

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** core state integrity

**Applies / trigger:** Output caps, search caps, overflow files or unavailable blobs

**Required evidence:** Raw byte ranges, scanned candidate count, overflow references and truncation flags. Also require the common evidence contract.

**Pass:** Required evidence was captured and examined completely for the asserted scope.

**Fail:** Complete inspection is asserted despite a known unread range or missing overflow artifact.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A complete decisive sub-result may support a narrow claim even when unrelated output is truncated

**Validation pair:** conforming - Decisive structured result retained; violating - capped search declares no evidence anywhere.

### EVD-11 - Oracle independence and tampering

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Tests, baselines, check configuration or evaluator permissions change

**Required evidence:** Protected oracle baseline, diff, authorization and runner access policy. Also require the common evidence contract.

**Pass:** The oracle used was authorized and its independence meets the gate's policy.

**Fail:** An agent weakens the gate, substitutes a result or writes protected verdict state.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Legitimate test changes are allowed when reviewed; content changes alone are not cheating

**Validation pair:** conforming - Reviewed regression-test update; violating - agent replaces assertions with unconditional success.

### EVD-12 - Reproducible verdict and receipt

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** core state integrity

**Applies / trigger:** Graph rebuild, export, adapter upgrade or disputed result

**Required evidence:** Immutable inputs, check/adapter/policy versions, parsed facts and rule trace. Also require the common evidence contract.

**Pass:** Replaying the same frozen inputs produces the same result and navigable receipts.

**Fail:** Identical inputs produce different deterministic states or a receipt points to unrelated bytes.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Model annotations must be frozen as inputs; re-running a model is not deterministic replay

**Validation pair:** conforming - Two rebuilds agree byte-for-byte; violating - model rerun silently changes a historical verdict.


## FUN: Functional behavior and contracts

Research context: [17], [33]. These are proposed checks, not claims of shipped coverage.

### FUN-01 - Build and type correctness

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Relevant source, compiler or configuration changes

**Required evidence:** Fresh compiler/type-checker results for required targets, flags and artifact. Also require the common evidence contract.

**Pass:** All specified targets compile and type-check with expected diagnostics.

**Fail:** A required target fails or prohibited diagnostics are emitted.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Compilation does not establish runtime semantics

**Validation pair:** conforming - All supported targets compile; violating - optional target used in release does not compile.

### FUN-02 - Public API contract

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Route, library API or schema changes

**Required evidence:** Versioned API schema, consumer fixtures and request/response traces. Also require the common evidence contract.

**Pass:** Required inputs, outputs and error forms satisfy the accepted contract.

**Fail:** A specified operation or consumer receives incompatible behavior.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Undocumented consumers remain outside demonstrated coverage

**Validation pair:** conforming - Old consumer passes compatibility probe; violating - required response field disappears.

### FUN-03 - Real persistence through the full workflow

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** User-visible create/update/delete work

**Required evidence:** Browser or client trace, API response, database assertion and reload/restart result. Also require the common evidence contract.

**Pass:** The accepted operation changes the intended durable state and survives the required reload.

**Fail:** UI reports success but state is absent, in the wrong store or lost after restart.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Use disposable test data and identify the actual environment

**Validation pair:** conforming - Create then reload preserves item; violating - UI-only mock claims save succeeded.

### FUN-04 - Boundary and malformed input behavior

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Parser, validator or numeric boundary changes

**Required evidence:** Contract, generated boundary cases and shrunk counterexamples. Also require the common evidence contract.

**Pass:** Observed boundary cases meet the accepted rejection or normalization rules.

**Fail:** A concrete case violates those rules or crashes the supported input boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Passing sampled cases does not cover all possible inputs

**Validation pair:** conforming - Empty and maximum-length inputs handled; violating - one-past-limit input corrupts output.

### FUN-05 - Error propagation and failure paths

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** New exception handling, catch blocks or integrations

**Required evidence:** Injected dependency failures, logs, outward response and resulting state. Also require the common evidence contract.

**Pass:** Specified failures remain visible and leave the defined state.

**Fail:** A caught error becomes false success or a required failure is silently discarded.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Expected recovery must be distinguished from masking

**Validation pair:** conforming - Database outage returns defined error; violating - catch block returns success with no write.

### FUN-06 - Time, numeric and locale semantics

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Time windows, money-like arithmetic, sorting or internationalization

**Required evidence:** Accepted rounding/timezone/locale contract and edge-case fixtures. Also require the common evidence contract.

**Pass:** Results agree with the specified semantics across the declared edge cases.

**Fail:** A reproducible rollover, precision or locale case violates the contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Domain rules require acceptance; floating-point use alone is not a defect

**Validation pair:** conforming - DST boundary preserves intended duration; violating - locale parsing changes a stored amount.

### FUN-07 - Feature configuration matrix

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Flags, editions, platforms or optional integrations change

**Required evidence:** Supported configuration manifest and selected interaction tests. Also require the common evidence contract.

**Pass:** Declared supported combinations satisfy their required behavior.

**Fail:** A supported tested combination breaks its contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Unexecuted combinations stay uncovered; pairwise testing is not exhaustive

**Validation pair:** conforming - Feature enabled and disabled both work; violating - disabled integration crashes startup.

### FUN-08 - Implemented behavior versus placeholder

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Completion claims and newly added routes/actions

**Required evidence:** Reachability map, source/diff, runtime probe and accepted implementation scope. Also require the common evidence contract.

**Pass:** Claimed production path reaches an implementation satisfying its criterion.

**Fail:** Claimed behavior reaches a stub, constant mock, no-op or unimplemented exception.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Scaffolding explicitly requested by the user is valid scaffolding

**Validation pair:** conforming - Action reaches tested service; violating - action only logs a message while marked complete.


## TST: Test validity and independence

Research context: [7], [17], [32], [37], [39]. These are proposed checks, not claims of shipped coverage.

### TST-01 - Tests discovered and actually executed

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Every test run

**Required evidence:** Runner discovery manifest, test IDs, shard results and skips. Also require the common evidence contract.

**Pass:** Required tests are discovered and execute, with approved exclusions visible.

**Fail:** Zero tests or an unintended subset is reported as the required test pass.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Some checks intentionally contain no tests; applicability must be explicit

**Validation pair:** conforming - Expected suite executes; violating - runner exits zero after discovering no tests.

### TST-02 - Meaningful assertions and independent expectations

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** New tests or claimed coverage

**Required evidence:** Test body, requirement link, expected-value provenance and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer confirms assertions could falsify the intended behavior independently of the implementation.

**Fail:** Reviewer identifies a vacuous assertion or expectation copied from the same faulty computation.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Static heuristics are triage; do not automatically accuse every shared helper

**Validation pair:** conforming - Assertion checks persisted result; violating - assertion only checks mock function called.

### TST-03 - Targeted mutation control

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** High-risk logic or suspiciously easy tests

**Required evidence:** Mutation operator, source diff, test outcomes and equivalent-mutant review. Also require the common evidence contract.

**Pass:** Required non-equivalent seeded faults are detected under the accepted control policy.

**Fail:** A required seeded behavioral fault survives without detection.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Equivalent mutants, tool timeouts and infrastructure failure need separate interpretation

**Validation pair:** conforming - Changed authorization branch is caught; violating - bypassing that branch leaves tests green.

### TST-04 - Regression reproduction

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** A bug fix is claimed

**Required evidence:** Unfixed baseline, minimal reproducer, fixed snapshot and the same oracle. Also require the common evidence contract.

**Pass:** The reproducer exposes the old defect and the fixed artifact satisfies the expected behavior.

**Fail:** The fixed artifact still exhibits the defect or the claimed reproduction never exercised it.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Historical state may be unavailable; report that limitation

**Validation pair:** conforming - Failing input breaks baseline and passes fix; violating - test passes both without exercising the defect.

### TST-05 - Property and metamorphic checks

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Serialization, transformation or algorithm changes

**Required evidence:** Accepted invariants, input generator, seed and minimized counterexample. Also require the common evidence contract.

**Pass:** No violation appears in the declared generated campaign.

**Fail:** A generated input violates an accepted property.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: An incorrect property can reject correct code; campaign success is bounded

**Validation pair:** conforming - Round trip preserves valid data; violating - serializer loses an allowed field.

### TST-06 - Differential reference check

**Priority:** P2 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Replacing parsers, algorithms or implementations

**Required evidence:** Independent reference version, input corpus and both outputs. Also require the common evidence contract.

**Pass:** Outputs agree under the accepted equivalence relation.

**Fail:** A reproducible difference violates that relation.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: The reference may be wrong; disputed semantics need review

**Validation pair:** conforming - Old and new encoders agree; violating - edge input yields divergent required output.

### TST-07 - Integration boundaries beyond mocks

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** External service, storage or queue integration work

**Required evidence:** Real compatible test service, request/response captures and side effects. Also require the common evidence contract.

**Pass:** Contract holds across the actual integration boundary.

**Fail:** Mocked tests pass while the real compatible dependency rejects or misinterprets the request.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Production-only policies require separately scoped evidence

**Validation pair:** conforming - Real database constraint exercised; violating - mock accepts a schema the database rejects.

### TST-08 - Flake and nondeterminism classification

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Inconsistent outcomes or concurrency-sensitive tests

**Required evidence:** Repeated identical-input runs, seeds, host context and failure signatures. Also require the common evidence contract.

**Pass:** Observed variability stays within an accepted flake policy with failures disclosed.

**Fail:** A required deterministic test changes outcome under controlled equivalent conditions.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Repetition estimates observed flakiness; it does not prove its absence

**Validation pair:** conforming - Stable controlled runs; violating - unexplained intermittent failure hidden by retry-to-green.

### TST-09 - Coverage relevance and negative cases

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** High coverage claims or risk-bearing feature changes

**Required evidence:** Changed branches, requirement/risk map, negative-case tests and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer confirms important success and denial/failure paths are exercised.

**Fail:** Material risk paths lack relevant tests despite a coverage claim.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Line coverage measures execution, not correctness or sufficient risk coverage

**Validation pair:** conforming - Unauthorized path tested; violating - 100 percent lines reached without checking authorization.

### TST-10 - Test isolation and contaminated state

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Suite order changes, parallel execution or cache reuse

**Required evidence:** Clean environment runs, isolated fixtures, reset receipts and order permutations. Also require the common evidence contract.

**Pass:** Required tests pass independently without relying on another run's leftovers.

**Fail:** Passing depends on leaked state, stale caches or execution order contrary to contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Real shared-service workflows may intentionally share state and need explicit isolation rules

**Validation pair:** conforming - Fresh database run passes; violating - only a warmed developer database passes.


## ARC: System architecture

Research context: [5], [6], [28], [29]. These are proposed checks, not claims of shipped coverage.

### ARC-01 - Allowed module and layer dependencies

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Imports, module boundaries or build graph change

**Required evidence:** Versioned allowed-edge rules, resolved dependency graph and exact violating edge. Also require the common evidence contract.

**Pass:** All resolved edges comply with the accepted architecture contract.

**Fail:** A concrete edge crosses a prohibited boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: No rule means advisory review; dynamic edges may be unresolved

**Validation pair:** conforming - Domain imports its port; violating - domain directly imports forbidden UI framework.

### ARC-02 - Dependency cycles

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Dependency graph changes

**Required evidence:** Resolved module graph, cycle path and approved baseline exemptions. Also require the common evidence contract.

**Pass:** No newly prohibited strongly connected component exists.

**Fail:** A concrete unapproved cycle violates the accepted boundary policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Cycles within explicitly allowed components are not automatically defects

**Validation pair:** conforming - Acyclic package graph; violating - A to B to C to A crosses the prohibited boundary.

### ARC-03 - Ownership and authoritative state

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** New services, caches or replicated business state

**Required evidence:** Accepted ownership map, read/write paths and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer confirms each critical state has explicit authority and synchronization rules.

**Fail:** Conflicting writable authorities lack an accepted reconciliation contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: This is architectural judgment until a concrete invariant can be tested

**Validation pair:** conforming - One ledger owns balances; violating - independent services both mutate balances without reconciliation.

### ARC-04 - Dependency direction and replaceable adapters

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Infrastructure or vendor integration changes

**Required evidence:** Accepted port/adapter rules, import graph and substitute-adapter tests. Also require the common evidence contract.

**Pass:** Required core behavior depends on accepted interfaces and passes the substitute contract.

**Fail:** A prohibited concrete dependency or incompatible adapter breaks the accepted contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not force abstraction into a small project without an accepted need

**Validation pair:** conforming - Storage adapter can be substituted; violating - core cannot load without forbidden vendor SDK.

### ARC-05 - Change coupling and fan-out review

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** A small feature causes widespread edits

**Required evidence:** Diff, normalized dependency deltas, historical baseline and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer accepts the change's coupling and explains material new dependencies.

**Fail:** Reviewer rejects unnecessary change propagation against the project's architecture goals.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: File count and smell count alone are not proof of poor design

**Validation pair:** conforming - Local feature changes its owned module; violating - same rule must be edited in many unrelated modules.

### ARC-06 - Service boundaries and failure isolation

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** New distributed call or shared dependency

**Required evidence:** Accepted availability contract, call graph and fault-injection traces. Also require the common evidence contract.

**Pass:** Specified failures remain within the allowed blast radius.

**Fail:** Injected dependency failure propagates beyond the agreed isolation boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A finite fault campaign does not prove universal resilience

**Validation pair:** conforming - Optional analytics outage preserves checkout; violating - analytics failure blocks checkout.

### ARC-07 - Capacity and topology assumptions

**Priority:** P2 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Scalability claims or topology changes

**Required evidence:** Accepted workload, deployment topology, capacity experiment and resource metrics. Also require the common evidence contract.

**Pass:** Measured system meets its defined capacity contract in the specified topology.

**Fail:** A reproducible run breaches the accepted capacity contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not extrapolate a laptop or toy dataset to a million users

**Validation pair:** conforming - Declared representative load meets target; violating - same declared load saturates a shared bottleneck.

### ARC-08 - Architecture decision consistency

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** A material design decision is introduced or reversed

**Required evidence:** Accepted decision record, alternatives, constraints, affected code and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer accepts the implementation's consistency with the active decision or an explicit revision.

**Fail:** Implementation contradicts an active decision without a recorded accepted change.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Architecture quality has tradeoffs; an AI opinion cannot be a hard gate

**Validation pair:** conforming - Documented local-first boundary preserved; violating - unreviewed remote dependency enters core.


## ALG: Algorithms and data structures

Research context: [5], [37]. These are proposed checks, not claims of shipped coverage.

### ALG-01 - Container choice for the actual workload

**Priority:** P2 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Lookup, insertion, deletion or traversal hotspots change

**Required evidence:** Operation distribution, representative cardinalities, profiles and alternative benchmark. Also require the common evidence contract.

**Pass:** Selected structure meets the accepted resource budget for its declared workload.

**Fail:** Measured operations breach that budget in a reproducible representative run.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A vector can beat a map at small sizes; syntax alone does not establish a wrong choice

**Validation pair:** conforming - Small vector meets measured budget; violating - repeated large linear lookup exceeds it.

### ALG-02 - Scaling growth and algorithmic blow-up

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Input-sensitive loops, joins or recursion changes

**Required evidence:** Input-size ladder, operation counters, time/memory samples and accepted scaling envelope. Also require the common evidence contract.

**Pass:** Measured growth remains within the accepted envelope across tested sizes.

**Fail:** A reproducible tested size or growth interval exceeds the envelope.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Empirical scaling is not a proof of asymptotic complexity

**Validation pair:** conforming - Indexed join scales within envelope; violating - nested scan exceeds envelope as data grows.

### ALG-03 - Equality, hashing and ordering laws

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Custom key, comparator or hash implementation changes

**Required evidence:** Accepted algebraic laws and property-test counterexamples. Also require the common evidence contract.

**Pass:** Tested values satisfy equality/hash consistency and required ordering laws.

**Fail:** A concrete input violates a required law.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Generated tests do not cover every value; partial ordering can be intentional

**Validation pair:** conforming - Equal keys have equal hashes; violating - comparator violates transitivity.

### ALG-04 - Deterministic ordering and tie-breaking

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Exports, sorting, aggregation or graph layout inputs change

**Required evidence:** Accepted ordering contract, repeated runs and tied/missing-value fixtures. Also require the common evidence contract.

**Pass:** Outputs preserve the required order and stable tie-breaking.

**Fail:** Equivalent inputs produce an order contrary to the contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: UI geometry may vary while semantic ordering remains correct

**Validation pair:** conforming - Chronological sessions with stable ties; violating - UUID order misrepresents time.

### ALG-05 - Queue, cache and collection bounds

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** New buffers, caches, retry queues or accumulators

**Required evidence:** Configured limits, overflow/eviction policy and sustained overload trace. Also require the common evidence contract.

**Pass:** Collections respect the accepted bounds and required overload semantics.

**Fail:** A measured collection exceeds its bound or loses required data outside the contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A configured limit is not evidence that runtime enforcement works

**Validation pair:** conforming - Queue applies tested backpressure; violating - producers grow an unbounded backlog.

### ALG-06 - Streaming and materialization

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Large-file, JSON, batch or export processing changes

**Required evidence:** Allocation profile, input-size experiment and accepted streaming contract. Also require the common evidence contract.

**Pass:** Peak retained data stays within the accepted bound while preserving required output.

**Fail:** Whole-input materialization or copies breach the measured bound.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Some algorithms legitimately require the whole input; accept that tradeoff explicitly

**Validation pair:** conforming - JSONL processed incrementally; violating - every raw payload is copied into a second full graph.

### ALG-07 - Arithmetic, indexing and recursion safety

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Size calculations, slices or recursive parsers change

**Required evidence:** Boundary corpus, sanitizer or checked-runtime results and minimized inputs. Also require the common evidence contract.

**Pass:** Tested inputs obey accepted bounds without prohibited overflow or depth failure.

**Fail:** A concrete input causes forbidden overflow, out-of-bounds access or stack exhaustion.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Saturating versus rejecting overflow is a domain contract

**Validation pair:** conforming - Maximum supported size is safe; violating - size multiplication wraps before allocation.

### ALG-08 - Semantic parity after optimization

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Optimization replaces a simpler implementation

**Required evidence:** Reference behavior, equivalence rule, differential/property tests and edge cases. Also require the common evidence contract.

**Pass:** Optimized behavior remains equivalent for the declared campaign and contract.

**Fail:** A counterexample changes required output or side effects.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Faster is irrelevant if semantics change; reference limitations remain visible

**Validation pair:** conforming - Batched operation preserves ordering; violating - optimization drops duplicate but meaningful events.


## DAT: Data integrity and persistence

Research context: [35], [36]. These are proposed checks, not claims of shipped coverage.

### DAT-01 - Schema constraints and invariant enforcement

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Schema or write-path changes

**Required evidence:** Migration, actual database schema and invalid-write probes. Also require the common evidence contract.

**Pass:** Accepted uniqueness, nullability and referential constraints reject invalid state.

**Fail:** An invalid state prohibited by the contract is accepted.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Application validation alone may be bypassed by alternate writers

**Validation pair:** conforming - Database rejects duplicate identity; violating - second writer bypasses UI-only uniqueness check.

### DAT-02 - Transaction atomicity

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Multi-step durable operations change

**Required evidence:** Transaction boundary, injected mid-operation failure and database snapshots. Also require the common evidence contract.

**Pass:** Operation commits all required effects or leaves the accepted rollback state.

**Fail:** A failure produces a forbidden partial durable state.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: External side effects require a separate reconciliation or outbox contract

**Validation pair:** conforming - Debit and credit roll back together; violating - only debit remains after a crash.

### DAT-03 - Migration compatibility and data preservation

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Schema migration or data backfill is proposed

**Required evidence:** Representative old database, migration logs, old/new client probes and invariant counts. Also require the common evidence contract.

**Pass:** Required data and supported client behavior survive the upgrade.

**Fail:** Migration loses required data or breaks a supported compatibility window.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A successful empty-database migration proves little about production data

**Validation pair:** conforming - Populated upgrade preserves records; violating - type conversion truncates real values.

### DAT-04 - Idempotent writes and duplicate delivery

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Retryable API, webhook or event consumer changes

**Required evidence:** Idempotency contract, duplicate requests/messages and final durable state. Also require the common evidence contract.

**Pass:** Repeated delivery has exactly the allowed additional effects.

**Fail:** Duplicate delivery creates a forbidden repeated effect.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Exactly-once claims require a precisely scoped system boundary

**Validation pair:** conforming - Same payment event applied once; violating - retry creates a second charge record.

### DAT-05 - Concurrent update consistency

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Shared mutable database rows or counters change

**Required evidence:** Controlled interleavings, transaction isolation and invariant assertions. Also require the common evidence contract.

**Pass:** Tested interleavings preserve the accepted consistency invariant.

**Fail:** A reproducible interleaving loses an update or creates a forbidden state.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Serial tests do not exercise contention; isolation semantics are database-specific

**Validation pair:** conforming - Two increments both persist; violating - last writer erases the first increment.

### DAT-06 - Query correctness and pagination

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Filters, joins, ordering or pagination change

**Required evidence:** Representative dataset, query parameters and complete expected result sets. Also require the common evidence contract.

**Pass:** Pages and filters return the accepted records without forbidden gaps, duplication or tenant mixing.

**Fail:** A concrete boundary or concurrent-change case violates the pagination contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Offset and cursor semantics differ; accept the intended behavior first

**Validation pair:** conforming - Stable cursor covers tied timestamps; violating - tied rows disappear between pages.

### DAT-07 - Backup, restore and recovery integrity

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Storage format, backup job or recovery code changes

**Required evidence:** Actual backup, clean restore, checksums and post-restore invariant probes. Also require the common evidence contract.

**Pass:** Restored system meets its accepted recovery and integrity criteria.

**Fail:** Backup cannot restore required state or silently loses committed data outside the recovery contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A backup command's exit code is not proof of recoverability

**Validation pair:** conforming - Clean restore passes data checks; violating - archive exists but lacks required sidecar data.

### DAT-08 - Retention, deletion and data minimization

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Retention policy, export or deletion feature changes

**Required evidence:** Accepted data lifecycle policy, storage inventory and deletion/archive probes. Also require the common evidence contract.

**Pass:** Observed lifecycle follows the accepted scope while preserving required audit metadata.

**Fail:** Expired/deleted data remains reachable contrary to policy or required history disappears silently.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Legal duties are outside this report; retention decisions require explicit project policy

**Validation pair:** conforming - Approved deletion removes all declared copies; violating - primary row disappears but export retains it.


## SEC: Application and supply-chain security

Research context: [8], [9], [21], [42]. These are proposed checks, not claims of shipped coverage.

### SEC-01 - Authentication and session boundaries

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Login, token validation or session changes

**Required evidence:** Accepted authentication contract, invalid/expired/revoked credentials and session traces. Also require the common evidence contract.

**Pass:** Required resources reject invalid credentials and honor session rules.

**Fail:** A concrete invalid credential gains access or a required revocation fails.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A bounded test matrix does not establish complete authentication security

**Validation pair:** conforming - Expired token denied; violating - expired token still accesses protected route.

### SEC-02 - Authorization and tenant isolation

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Protected route, object access or tenant query changes

**Required evidence:** Role/tenant access matrix, negative requests and database side effects. Also require the common evidence contract.

**Pass:** Every tested operation enforces its accepted actor/object/action permission.

**Fail:** Unauthorized actor reads or mutates a forbidden object in a reproducible test.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Authentication is not object authorization; broad matrix completeness needs review

**Validation pair:** conforming - Tenant A denied tenant B record; violating - direct object ID bypasses tenant filter.

### SEC-03 - Injection and unsafe interpretation

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Untrusted data reaches SQL, commands, templates or browser output

**Required evidence:** Data-flow finding plus minimized safe test showing the forbidden interpretation. Also require the common evidence contract.

**Pass:** Required boundaries preserve data/code separation under the accepted probes.

**Fail:** A reproducible input crosses a prohibited interpreter boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Static findings without reachable flow may remain advisory; never run harmful probes on live data

**Validation pair:** conforming - Parameterized query keeps input literal; violating - input changes query structure in a disposable test.

### SEC-04 - Path, archive and file boundary safety

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** File read/write, upload, extraction or symlink handling changes

**Required evidence:** Accepted filesystem root, adversarial path fixtures and actual resolved targets. Also require the common evidence contract.

**Pass:** Operations remain within the allowed roots and file-type policy.

**Fail:** A concrete path or archive entry escapes the permitted boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Platform-specific path resolution and time-of-check races need appropriate adapters

**Validation pair:** conforming - Archive entry rejected outside root; violating - relative traversal writes outside test root.

### SEC-05 - Outbound request and redirect restrictions

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** URL fetching, webhooks or proxy behavior changes

**Required evidence:** Accepted egress policy, DNS/redirect fixtures and network observations. Also require the common evidence contract.

**Pass:** Requests including redirects stay within permitted destinations and schemes.

**Fail:** A controlled input reaches a prohibited destination in the test environment.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: String filtering alone does not establish final network destination

**Validation pair:** conforming - Blocked redirect stays blocked; violating - allowed URL redirects to forbidden internal endpoint.

### SEC-06 - Secrets and sensitive logs

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Diff, generated artifact, log or export changes

**Required evidence:** Secret scanner findings, redaction rules, access classification and reviewed matches. Also require the common evidence contract.

**Pass:** No confirmed prohibited secret exposure appears in the inspected scope.

**Fail:** A verified sensitive value is exposed where policy forbids it.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: False positives require handling without printing secret values into receipts

**Validation pair:** conforming - Receipt shows redacted fingerprint; violating - export contains a live credential.

### SEC-07 - Dependency existence and exact identity

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** A dependency or import is added or changed

**Required evidence:** Registry metadata snapshot, exact namespace/version, lockfile and resolved artifact digest. Also require the common evidence contract.

**Pass:** Declared package identity/version exists in the configured trusted registry and resolves as expected.

**Fail:** A dependency is nonexistent, mismatched or resolves to a different unapproved identity.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Existence does not mean trustworthy; stale offline metadata yields limited or unknown coverage

**Validation pair:** conforming - Exact accepted package resolves; violating - invented package name or wrong namespace is proposed.

### SEC-08 - Dependency vulnerabilities and provenance

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Lockfile changes or vulnerability metadata refreshes

**Required evidence:** SBOM/lock resolution, artifact identity, advisory database version and policy. Also require the common evidence contract.

**Pass:** Resolved dependencies have no policy-blocking matched known finding in the stated fresh dataset.

**Fail:** A concrete affected version violates an accepted vulnerability policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Clean known-vulnerability scans do not exclude zero-days; offline freshness must be explicit

**Validation pair:** conforming - Unaffected locked version clears defined policy; violating - affected version matches a blocking advisory.

### SEC-09 - Install scripts and build-time permissions

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** New dependency lifecycle script or build step

**Required evidence:** Script diff, requested capabilities and sandbox execution trace. Also require the common evidence contract.

**Pass:** Install/build activity stays within its accepted filesystem, process and network capabilities.

**Fail:** Observed script activity exceeds those capabilities.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not install an unknown package merely to check whether it exists

**Validation pair:** conforming - Package build stays in sandbox; violating - lifecycle script accesses forbidden credential path.

### SEC-10 - Secure defaults and browser protections

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** CORS, CSRF, cookie, transport or debug settings change

**Required evidence:** Accepted environment-specific policy and negative browser/HTTP probes. Also require the common evidence contract.

**Pass:** Deployed test configuration satisfies the selected protections.

**Fail:** A reproducible request or inspected effective setting violates a required protection.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Requirements vary by application; do not impose irrelevant browser controls on non-web code

**Validation pair:** conforming - Production cookie policy enforced; violating - development debug endpoint exposed in release configuration.

### SEC-11 - Cryptographic construction and randomness review

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** Cryptography, credential storage or random token generation changes

**Required evidence:** Threat model, approved library/protocol, configuration and specialist review. Also require the common evidence contract.

**Pass:** Reviewer accepts the selected standard construction and its usage for the stated threat model.

**Fail:** Reviewer identifies prohibited custom cryptography, insecure randomness or incorrect key handling.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Algorithm names and passing tests alone do not prove cryptographic security

**Validation pair:** conforming - Approved protocol used with reviewed parameters; violating - homegrown reversible password storage.

### SEC-12 - Privacy boundaries and data export

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Telemetry, model provider, export or synchronization changes

**Required evidence:** Accepted data-egress policy, effective configuration and observed network/file destinations. Also require the common evidence contract.

**Pass:** Observed sensitive data flows stay within the accepted local/remote boundaries.

**Fail:** Captured flow violates an explicit privacy boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Absence of observed traffic covers only the instrumented interval and channels

**Validation pair:** conforming - Offline core operates without egress; violating - core uploads transcript content unexpectedly.


## PER: Performance and resource use

Research context: [5], [30], [31], [35]. These are proposed checks, not claims of shipped coverage.

### PER-01 - Latency and throughput regression

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Performance-critical path changes

**Required evidence:** Protected baseline, representative workload, repetitions, p50/p95/p99 and host conditions. Also require the common evidence contract.

**Pass:** Measured distribution and throughput satisfy the accepted comparison and absolute budget.

**Fail:** A reproducible controlled result breaches an accepted budget beyond its tolerance.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: One timing or incomparable hardware is inconclusive; disclose sampling uncertainty

**Validation pair:** conforming - Comparable repeated runs meet p95 target; violating - same workload consistently breaches it.

### PER-02 - Peak memory and retained growth

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Allocation-heavy or long-lived code changes

**Required evidence:** Process-tree RSS, managed/native heap where available, workload phases and soak profile. Also require the common evidence contract.

**Pass:** Peak and post-cycle retained memory meet accepted budgets.

**Fail:** Reproducible peak or retained-growth behavior exceeds the accepted contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Caches, allocator retention, mapped space and GC timing are not automatically leaks

**Validation pair:** conforming - Steady-state reuse stays bounded; violating - retained objects grow across identical completed cycles.

### PER-03 - Allocation and copy amplification

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Hot paths, object mapping or serialization changes

**Required evidence:** Allocation count/bytes, copy profile and representative input sizes. Also require the common evidence contract.

**Pass:** Measured allocation/copy cost stays within the accepted budget.

**Fail:** A reproducible changed path exceeds the allocation/copy budget.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Source-level copy counts can differ from optimized runtime behavior

**Validation pair:** conforming - Borrowed slices meet budget; violating - repeated full-payload copies breach it.

### PER-04 - Resource lifetime and cleanup

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** File, socket, subscription, process or connection ownership changes

**Required evidence:** Resource counters before/after repeated operations, cancellation and error paths. Also require the common evidence contract.

**Pass:** Resources return to the allowed steady-state range after all required paths.

**Fail:** A repeatable path leaks handles, children or connections beyond the accepted range.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Pooling may intentionally retain resources within an explicit bound

**Validation pair:** conforming - Cancelled request releases connection; violating - each cancellation leaves one child process alive.

### PER-05 - Query count, N+1 and execution plans

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Database access or list rendering changes

**Required evidence:** Per-operation query counts, representative rows, plan statistics and accepted query budget. Also require the common evidence contract.

**Pass:** Query count and measured plan cost meet the workload budget.

**Fail:** A reproduced operation exceeds the allowed query/resource cost.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: EXPLAIN ANALYZE executes SQL; use a disposable database for mutating statements

**Validation pair:** conforming - List query count remains bounded; violating - one extra query appears for every row.

### PER-06 - Event-loop or UI-thread blocking

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Synchronous work enters interactive or async paths

**Required evidence:** Responsiveness traces, long-task spans and accepted interaction budget. Also require the common evidence contract.

**Pass:** Observed blocking remains below the accepted limit during the declared workload.

**Fail:** A reproducible blocking interval breaks the interaction budget.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Short synthetic runs can miss large-payload stalls

**Validation pair:** conforming - Large import keeps UI responsive; violating - synchronous parse freezes interactions.

### PER-07 - Backpressure and admission control

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Producer/consumer, upload or job scheduling changes

**Required evidence:** Rate/load experiment, queue metrics and accepted overload policy. Also require the common evidence contract.

**Pass:** Excess work is bounded, deferred or rejected according to contract.

**Fail:** Overload causes unbounded growth or forbidden loss/acceptance.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A nominal request-rate limit does not prove downstream capacity control

**Validation pair:** conforming - Queue saturates and rejects predictably; violating - accepted backlog grows until out of memory.

### PER-08 - Cancellation, timeout and retry budgets

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Long operations or retries change

**Required evidence:** Deadline policy, cancellation propagation and process/network traces. Also require the common evidence contract.

**Pass:** Work stops or unwinds within the accepted limits and retry budget.

**Fail:** Cancelled or expired work continues consuming resources beyond the contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Some external actions are non-cancellable; surface that limitation explicitly

**Validation pair:** conforming - Cancellation reaches child process; violating - parent stops but child continues indefinitely.

### PER-09 - Payload, bundle and artifact growth

**Priority:** P2 · **Method:** controlled measurement · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Dependencies, assets, generated files or packaging change

**Required evidence:** Reachable bundle composition, compressed/uncompressed sizes and accepted size budget. Also require the common evidence contract.

**Pass:** Required artifacts stay within the agreed size and loading budget.

**Fail:** A measured artifact exceeds its accepted size/loading budget.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Source whitespace is not automatically shipped or a meaningful runtime cost

**Validation pair:** conforming - Tree-shaken build stays within budget; violating - unused SDK dominates delivered bundle.

### PER-10 - Cache correctness and bounded benefit

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Cache keys, invalidation, TTL or capacity changes

**Required evidence:** Key policy, cold/warm traces, invalidation probes and memory/hit metrics. Also require the common evidence contract.

**Pass:** Cache preserves accepted semantics and resource limits in tested scenarios.

**Fail:** Stale, cross-user or incorrectly keyed data violates the contract, or bounds fail.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A high hit rate does not establish correctness or net benefit

**Validation pair:** conforming - Tenant-aware cache invalidates on write; violating - tenant omitted from cache key.


## REL: Concurrency and resilience

Research context: [24], [38]. These are proposed checks, not claims of shipped coverage.

### REL-01 - Data races and unsafe interleavings

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Concurrent shared-state or synchronization changes

**Required evidence:** Instrumented race report or bounded model trace and accepted invariant. Also require the common evidence contract.

**Pass:** No violation appears in the declared instrumented campaign.

**Fail:** A reproducible race or explored interleaving violates the accepted invariant.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Uninstrumented operations and unexplored schedules remain uncovered

**Validation pair:** conforming - Instrumented lock protects state; violating - controlled interleaving loses a write.

### REL-02 - Deadlock, starvation and lock contention

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Lock ordering or shared resource acquisition changes

**Required evidence:** Acquisition graph, bounded contention run and liveness budget. Also require the common evidence contract.

**Pass:** Operations make required progress under the tested contention model.

**Fail:** A reproducible cycle or starvation exceeds the accepted progress limit.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Timeout can indicate slow infrastructure rather than a proven deadlock

**Validation pair:** conforming - Stable lock order completes; violating - opposing lock order reproduces a cycle.

### REL-03 - Retry storms and circuit behavior

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Retry policy or dependency protection changes

**Required evidence:** Failure injection, attempt counts, jitter/backoff settings and recovery trace. Also require the common evidence contract.

**Pass:** Retry traffic and recovery stay within the accepted policy.

**Fail:** Failure amplification or retry volume breaches the accepted limit.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Retries are not inherently wrong; classify idempotency and recovery together

**Validation pair:** conforming - Outage produces bounded attempts; violating - every layer multiplies retries.

### REL-04 - Partial failure and compensating actions

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Multi-service workflow or saga changes

**Required evidence:** Failure-point matrix, side-effect ledger and reconciliation results. Also require the common evidence contract.

**Pass:** Each tested partial failure reaches an accepted terminal or recoverable state.

**Fail:** A tested failure leaves an unreconciled state prohibited by the contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Compensation itself can fail and requires its own observable status

**Validation pair:** conforming - Reservation released after payment failure; violating - reservation remains permanently stranded.

### REL-05 - Duplicate, reordered and delayed events

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Event-driven state machine changes

**Required evidence:** Delivery permutations, version/sequence metadata and invariant assertions. Also require the common evidence contract.

**Pass:** State remains valid under the declared delivery guarantees.

**Fail:** A allowed delivery order or delay produces a forbidden state.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not assume exactly-once ordered delivery without a real contract

**Validation pair:** conforming - Older update ignored by version; violating - delayed old event overwrites newer state.

### REL-06 - Crash consistency and atomic file updates

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Local configuration, ledger or checkpoint writing changes

**Required evidence:** Controlled interruption points, fsync/rename policy and restart reads. Also require the common evidence contract.

**Pass:** Restart sees an accepted old or new state with required durability.

**Fail:** Crash or competing writer exposes a torn or forbidden lost state.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Atomic rename and durability are distinct and filesystem-specific

**Validation pair:** conforming - Crash preserves old or new config; violating - truncated file becomes the only configuration.

### REL-07 - Graceful shutdown and drain

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Daemon, worker or service lifecycle changes

**Required evidence:** Signal trace, in-flight work accounting and post-restart state. Also require the common evidence contract.

**Pass:** Shutdown meets the accepted drain/cancel/recovery contract.

**Fail:** Accepted work disappears or resources persist outside that contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Force-kill behavior differs from graceful shutdown and needs separate cases

**Validation pair:** conforming - Inflight task drains or resumes; violating - shutdown acknowledges then loses it.

### REL-08 - Recovery after collector or tool failure

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Agent runtime, watcher or verifier crashes/restarts

**Required evidence:** Checkpoint, worktree state, run IDs, reconnect handshake and replay ledger. Also require the common evidence contract.

**Pass:** Recovery preserves work and marks interrupted evidence accurately without double-applying it.

**Fail:** Restart loses edits, duplicates effects or promotes an interrupted run to pass.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Infrastructure failure must remain distinct from a product test failure

**Validation pair:** conforming - Interrupted run resumes as incomplete; violating - crash is recorded as successful verification.


## UX: User experience and accessibility

Research context: [16], [33], [41]. These are proposed checks, not claims of shipped coverage.

### UX-01 - Reachable controls perform their promised action

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** New button, form, link or navigation flow

**Required evidence:** Real browser/client interaction, resulting route/API calls and required side effects. Also require the common evidence contract.

**Pass:** Each declared control completes its accepted action or shows its defined outcome.

**Fail:** A required control is dead, miswired or reports success without the required effect.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Decorative elements need not be interactive; use the accepted product contract

**Validation pair:** conforming - Save button persists data; violating - save button only changes local label.

### UX-02 - Loading, empty, error and retry states

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Async views or data-fetching behavior changes

**Required evidence:** Delayed, empty and failing responses plus browser assertions. Also require the common evidence contract.

**Pass:** Required states are visible, accurate and recover according to contract.

**Fail:** Spinner never resolves, empty data misleads, or retry cannot recover a tested failure.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Visual clarity beyond explicit criteria needs human judgment

**Validation pair:** conforming - Failure shows actionable retry; violating - network error leaves a blank screen indefinitely.

### UX-03 - Accessibility semantics and keyboard flow

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Interactive UI changes

**Required evidence:** Automated findings, keyboard/screen-reader test receipts and selected accepted criteria. Also require the common evidence contract.

**Pass:** The selected criteria pass across the declared workflows.

**Fail:** A reproducible required interaction violates an accepted accessibility criterion.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Automated scans cover only part of accessibility; unperformed manual checks stay unknown

**Validation pair:** conforming - Dialog traps/restores focus correctly; violating - keyboard user cannot close it.

### UX-04 - Responsive layout and overflow

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Layout, long content or localization changes

**Required evidence:** Accepted viewport/content matrix, screenshots and geometry assertions. Also require the common evidence contract.

**Pass:** Required content and controls remain usable in all tested supported layouts.

**Fail:** A supported tested layout clips or hides required functionality.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Device/browser combinations outside the matrix are unverified

**Validation pair:** conforming - Long translated label stays usable; violating - primary action falls off a supported narrow viewport.

### UX-05 - Visual hierarchy and design quality

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** New screen, generated layout or major visual change

**Required evidence:** Accepted design intent, rendered screens and calibrated human rubric. Also require the common evidence contract.

**Pass:** Reviewer accepts hierarchy, clarity and coherence for the target task.

**Fail:** Reviewer identifies a specific material mismatch with the accepted design intent.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Aesthetic quality is not an objective model-confidence score

**Validation pair:** conforming - Reviewer approves clear task flow; violating - reviewer finds primary and destructive actions indistinguishable.

### UX-06 - User-data preservation during interaction

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Forms, autosave, navigation or undo changes

**Required evidence:** Interaction sequence with interruption, reload and recovery assertions. Also require the common evidence contract.

**Pass:** User input survives or is discarded only as the accepted interaction contract specifies.

**Fail:** A tested navigation/error silently loses data contrary to contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Retention must respect privacy choices and explicit discard actions

**Validation pair:** conforming - Draft restored after temporary outage; violating - retry clears completed form without notice.

### UX-07 - Truthful status, counts and receipts

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Dashboard, badge, progress or evidence UI changes

**Required evidence:** Backend graph/result snapshot, rendered values and receipt navigation. Also require the common evidence contract.

**Pass:** Visible counts/states exactly reflect the displayed scope and authoritative data.

**Fail:** UI silently hides failures, reconciles inconsistent counts or presents a false completion state.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Pagination is allowed when totals and undisplayed scope are explicit

**Validation pair:** conforming - Filtered count labeled correctly; violating - partial graph displayed as the complete project.

### UX-08 - Browser and device compatibility

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Platform-sensitive UI or client API changes

**Required evidence:** Supported browser/device versions, workflow traces and feature-fallback assertions. Also require the common evidence contract.

**Pass:** Required workflows work on each tested supported target.

**Fail:** A specified supported target lacks required behavior or fallback.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Emulation is not identical to a physical device; show coverage provenance

**Validation pair:** conforming - Supported older browser gets fallback; violating - unsupported API silently breaks a promised target.


## DEL: Build, deployment and operations

Research context: [10], [14], [22], [34]. These are proposed checks, not claims of shipped coverage.

### DEL-01 - Reproducible clean build and artifact identity

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Release candidate or dependency/toolchain changes

**Required evidence:** Clean build inputs, pinned toolchain, output digests and provenance manifest. Also require the common evidence contract.

**Pass:** Build completes with the declared inputs and produces the expected identified artifact.

**Fail:** A required clean build fails or artifact provenance points to other inputs.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Byte reproducibility is a separate accepted requirement; timestamps may legitimately vary

**Validation pair:** conforming - Clean checkout builds required target; violating - build relies on a local untracked file.

### DEL-02 - Effective environment configuration

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Environment variables, flags or deployment target changes

**Required evidence:** Validated configuration schema, effective redacted values and environment-specific probes. Also require the common evidence contract.

**Pass:** Effective settings satisfy the accepted contract for the named environment.

**Fail:** Missing or wrong effective configuration breaks a required setting or behavior.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A checked-in template does not prove the running process loaded it

**Validation pair:** conforming - Production process uses required endpoint; violating - tests accidentally target a developer service.

### DEL-03 - Build-script error handling and dependency pinning

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** CI, Dockerfile, shell or build dependency changes

**Required evidence:** Parsed script, lock/pin policy and injected failing-command run. Also require the common evidence contract.

**Pass:** Scripts preserve required failures and resolve dependencies within accepted pinning policy.

**Fail:** Failure is masked or a dependency floats contrary to the accepted build policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Heuristic smells need confirmation when shell semantics are ambiguous

**Validation pair:** conforming - Pipeline propagates failed compiler status; violating - trailing echo hides failure.

### DEL-04 - Deployment identity and health

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** A deployment is claimed complete

**Required evidence:** Deployment operation ID, running artifact digest, target environment and health probe. Also require the common evidence contract.

**Pass:** The intended artifact is running on the intended target and meets the defined readiness criteria.

**Fail:** Wrong artifact/target is observed or a required readiness probe fails.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A successful deploy command alone does not establish healthy behavior

**Validation pair:** conforming - Runtime digest matches release and probe passes; violating - control plane accepted job but old code remains live.

### DEL-05 - Rollback or forward-recovery viability

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Release changes compatibility or persistent state

**Required evidence:** Accepted recovery strategy, actual rehearsal and restored service/data checks. Also require the common evidence contract.

**Pass:** Rehearsed recovery meets the declared integrity and service targets.

**Fail:** Required recovery path fails or corrupts state in rehearsal.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Rollback is not always safe after a migration; an accepted forward repair may be correct

**Validation pair:** conforming - Compatible rollback restores service; violating - downgraded binary cannot read migrated data.

### DEL-06 - Observability for critical outcomes

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** New service, job or critical workflow

**Required evidence:** Accepted telemetry contract, induced failure and correlated logs/metrics/traces. Also require the common evidence contract.

**Pass:** Critical outcomes and failures produce the required attributable, privacy-safe telemetry.

**Fail:** A required failure is invisible or cannot be linked to its operation/artifact.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: More logs are not automatically better; avoid secrets and uncontrolled cardinality

**Validation pair:** conforming - Failed job trace links to run ID; violating - only an unrelated success log is emitted.

### DEL-07 - Permissions and infrastructure exposure

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Infrastructure policy, service account or network rule changes

**Required evidence:** Accepted capability policy, effective deployment configuration and denial probes. Also require the common evidence contract.

**Pass:** Observed infrastructure permissions stay within the accepted scope.

**Fail:** A granted or exercised capability exceeds a required boundary.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Static configuration may differ from effective cloud state; missing read access means unknown

**Validation pair:** conforming - Service denied forbidden bucket; violating - wildcard permission exposes unrelated data.

### DEL-08 - Release documentation and operational handoff

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** Release or operational ownership transfer

**Required evidence:** Actual behavior/configuration, migration notes, runbook and reviewer walkthrough. Also require the common evidence contract.

**Pass:** Reviewer can perform the required supported operation and recovery using the handoff.

**Fail:** Material required steps are missing, wrong or describe obsolete behavior.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Documentation completeness is scoped to accepted operational responsibilities

**Validation pair:** conforming - Fresh operator follows verified restore steps; violating - runbook refers to removed command.


## MNT: Maintainability and unnecessary work

Research context: [5], [6], [40]. These are proposed checks, not claims of shipped coverage.

### MNT-01 - Unused code, exports and dependencies

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** Dependency or public-entry-point changes

**Required evidence:** Configured reachability analysis, dynamic entry-point inventory and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer confirms retained items are used or intentionally supported.

**Fail:** Reviewer confirms unreachable unnecessary items against the project's maintenance policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Dynamic imports, plugins and public libraries defeat naive unused-code conclusions; never auto-delete

**Validation pair:** conforming - Plugin entry point is retained; violating - abandoned private module confirmed unreachable.

### MNT-02 - Duplication and unnecessary abstraction

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** Generated helpers, layers or repeated logic grow

**Required evidence:** Diff, duplicate-pattern evidence, call graph and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer accepts duplication or abstraction as justified by current constraints.

**Fail:** Reviewer identifies specific unnecessary maintenance cost against an accepted simplicity goal.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not impose universal DRY or abstraction counts; small duplication may be clearer

**Validation pair:** conforming - Two simple distinct cases remain clear; violating - many wrappers obscure one trivial operation.

### MNT-03 - Complexity and comprehensibility

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** Large or intricate functions/classes change

**Required evidence:** Normalized complexity deltas, changed control flow and reviewer walkthrough. Also require the common evidence contract.

**Pass:** Reviewer can explain the relevant invariants and accepts the complexity tradeoff.

**Fail:** Reviewer identifies unmanageable complexity or hidden invariant violations needing redesign.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A cyclomatic threshold is a policy choice, not a universal quality law

**Validation pair:** conforming - Complex parser has explicit states; violating - ordinary path has unexplained nested flag interactions.

### MNT-04 - Temporary artifacts and debug residue

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Diff, packaging or release candidate changes

**Required evidence:** Accepted generated-file policy, artifact manifest and inspected matches. Also require the common evidence contract.

**Pass:** No prohibited temporary, debug or disposable item enters the released scope.

**Fail:** A confirmed prohibited artifact is packaged or exposed.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Temporary local files outside the released scope are not automatically a product defect

**Validation pair:** conforming - Local scratch excluded from bundle; violating - debug dump containing real data ships.

### MNT-05 - Comments and documentation agree with behavior

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** Documented contract or adjacent implementation changes

**Required evidence:** Relevant text, current behavior probes and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer confirms material documented claims remain accurate for the stated version.

**Fail:** A concrete material claim contradicts current observed behavior.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not spend a model pass policing harmless wording on every save

**Validation pair:** conforming - Comment describes actual retry bound; violating - docs promise unlimited size despite enforced cap.

### MNT-06 - Deprecated or incompatible API usage

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Dependency upgrades or new API calls

**Required evidence:** Pinned dependency documentation/type metadata and compatibility build/run. Also require the common evidence contract.

**Pass:** Used APIs exist and work under the declared supported versions.

**Fail:** An absent, removed or incompatible API breaks the accepted version contract.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Deprecation alone is not failure unless forbidden by policy or migration requirements

**Validation pair:** conforming - Pinned version exposes called method; violating - generated call belongs to another version.

### MNT-07 - Change isolation and rollback readability

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** Feature diff expands or unrelated refactors enter it

**Required evidence:** Accepted task scope, grouped diff and reviewer decision. Also require the common evidence contract.

**Pass:** Reviewer can isolate the intended behavior change and its recovery implications.

**Fail:** Unrelated changes prevent meaningful review against the accepted task.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Broad changes may be justified by a genuinely cross-cutting requirement

**Validation pair:** conforming - Focused behavioral change is reviewable; violating - formatting rewrite obscures a security fix.

### MNT-08 - License and attribution policy metadata

**Priority:** P2 · **Method:** human adjudicated review · **Cost:** S · **Gate:** advisory until human decision

**Applies / trigger:** Dependency, generated asset or external code is introduced

**Required evidence:** Provenance, declared licenses and accepted organization policy. Also require the common evidence contract.

**Pass:** Authorized reviewer accepts provenance and license metadata under the project policy.

**Fail:** Required provenance is missing or reviewer rejects the material under that policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: This is inventory and policy review, not a legal determination or guarantee of originality

**Validation pair:** conforming - Known source and approved license recorded; violating - copied asset has no traceable origin.


## AGT: Agent workflow and control

Research context: [15], [16], [18], [19], [20], [25], [26], [27]. These are proposed checks, not claims of shipped coverage.

### AGT-01 - Instruction-source and prompt-injection boundaries

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Repository text, web content or tool output enters agent context

**Required evidence:** Instruction provenance, capability policy and controlled hostile-content probes. Also require the common evidence contract.

**Pass:** Untrusted content cannot grant forbidden capabilities or alter protected verifier policy.

**Fail:** A controlled untrusted instruction causes an observed policy-boundary violation.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Injection detection cannot be complete; enforce capabilities outside the model

**Validation pair:** conforming - Log text cannot rewrite gate policy; violating - log instruction changes protected check configuration.

### AGT-02 - Tool permissions and approval enforcement

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Tool invocation, permission change or adapter upgrade

**Required evidence:** Accepted capability scope, actual runner authorization decision and action receipt. Also require the common evidence contract.

**Pass:** Observed actions remain within persistent user authorization and enforced capability bounds.

**Fail:** Action exceeds its accepted authority or bypasses a required approval mechanism.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not infer new permissions from agent prose; user intent can already authorize routine work

**Validation pair:** conforming - Authorized edit succeeds; violating - unrelated external write occurs without authority.

### AGT-03 - Delegated work needs artifact evidence

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Child agent reports completion

**Required evidence:** Delegation/task ID, returned artifact snapshot and independent applicable check results. Also require the common evidence contract.

**Pass:** Parent verification relies on admissible artifact results for the delegated criterion.

**Fail:** A child-agent completion message alone is promoted to verified implementation.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A task notification can prove that a report was received, not that its claim is true

**Validation pair:** conforming - Schema artifact passes validation; violating - child merely says strict validation is complete.

### AGT-04 - Context and requirement continuity

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Compaction, session resume or model/harness change

**Required evidence:** Frozen accepted requirements, unresolved obligations, decision revisions and resumed working scope. Also require the common evidence contract.

**Pass:** Resumed work retains the applicable accepted constraints and unresolved obligations.

**Fail:** A concrete active obligation or constraint is lost or contradicted after handoff.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Do not need private chain-of-thought; track observable decisions and actions

**Validation pair:** conforming - Resume preserves pending auth work; violating - compaction drops it from the task state.

### AGT-05 - No-progress loops and bounded autonomy

**Priority:** P1 · **Method:** controlled measurement · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Repeated tool calls, retries or large token/time spend

**Required evidence:** Operation fingerprints, artifact/result deltas and accepted progress/resource policy. Also require the common evidence contract.

**Pass:** Work stays within the accepted budget or produces the required measurable progress.

**Fail:** A measured loop breaches the policy without relevant progress.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Repeated investigation may be legitimate; budget breaches are not proof of deception

**Validation pair:** conforming - Repeated failure escalates with receipt; violating - identical failing command repeats beyond budget.

### AGT-06 - Accepted, queued, running and completed task states

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Scheduling, delegation or job status changes

**Required evidence:** Scheduler acknowledgment, first execution event, heartbeat and terminal outcome. Also require the common evidence contract.

**Pass:** Each displayed lifecycle state has its required observed transition.

**Fail:** Task is displayed as running/completed without the corresponding execution evidence.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A stalled scheduler is infrastructure trouble, not automatically a coding failure

**Validation pair:** conforming - Queued job remains queued until first action; violating - acceptance alone becomes completed.

### AGT-07 - Tool/model/harness version drift

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** S · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Client, adapter, model, prompt policy or runtime updates

**Required evidence:** Version/settings fingerprint and adapter conformance/canary suite. Also require the common evidence contract.

**Pass:** New combination is identified and satisfies required compatibility probes.

**Fail:** A known incompatible format/setting breaks a required check while old confidence is retained.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Same model name can hide provider changes; record available immutable identifiers and uncertainty

**Validation pair:** conforming - Updated format passes canary; violating - old parser silently drops new result blocks.

### AGT-08 - Evaluation manipulation and result laundering

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Agent attempts to write check outputs, baselines or receipts

**Required evidence:** Protected result channel, file/network access decisions and cross-channel consistency. Also require the common evidence contract.

**Pass:** Only authorized verifier processes can publish admissible results for protected gates.

**Fail:** Agent-controlled material is accepted through a protected result channel or a forged run identity.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Hashing or signing inside the agent's own privilege boundary cannot prove independence

**Validation pair:** conforming - Agent-supplied fake pass rejected; violating - forged result file is ingested as a trusted run.

### AGT-09 - Workspace identity and concurrent agent coordination

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Multiple worktrees, editors or agents act on a project

**Required evidence:** Workspace inode/path identity, branch/content snapshots, write ownership and merge/conflict events. Also require the common evidence contract.

**Pass:** Actions and results bind to the right workspace and preserve concurrent accepted edits.

**Fail:** Work is attributed to the wrong tree or a reproducible conflicting write loses another accepted change.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Path strings alone may alias via symlinks or moved roots; use supported identity mechanisms

**Validation pair:** conforming - Two isolated worktrees keep separate results; violating - pass from one tree verifies edits in another.

### AGT-10 - Verifier failure, budget exhaustion and abstention

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** XS · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Parser failure, resource cap, unavailable tool or cancelled verification

**Required evidence:** Verifier run state, error classification, resource measurements and coverage ledger. Also require the common evidence contract.

**Pass:** Incomplete verification is visibly unknown/blocked and preserves collected evidence.

**Fail:** Verifier failure, skipped work or resource exhaustion is rendered as pass or silent completion.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Resource limits should control verifier cost without making the user's product appear faulty

**Validation pair:** conforming - OOM produces blocked check with receipt; violating - failed checker becomes all-clear status.


## AIP: Products that themselves use AI

Research context: [17], [43], [44]. These are proposed checks, not claims of shipped coverage.

### AIP-01 - Claim grounding and source entailment

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** M · **Gate:** advisory until human decision

**Applies / trigger:** An AI feature generates factual answers or citations

**Required evidence:** Atomic claims, retrieved source snapshots, exact supporting passages and calibrated reviewer decisions. Also require the common evidence contract.

**Pass:** Reviewer confirms each required factual claim is supported in scope by its cited evidence.

**Fail:** A concrete claim invents a source, contradicts it or claims support the source does not supply.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A working URL or text overlap does not prove entailment; judge-only decisions stay advisory

**Validation pair:** conforming - Source actually supports stated version; violating - real citation says the opposite of the answer.

### AIP-02 - Retrieval relevance, permissions and freshness

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Retrieval corpus, index, embedding model or access policy changes

**Required evidence:** Versioned corpus/index, labeled retrieval cases, access matrix and retrieval traces. Also require the common evidence contract.

**Pass:** Declared retrieval criteria and access rules hold for the evaluated cases.

**Fail:** Required source is missed under the accepted test or unauthorized/stale content violates policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: No retriever guarantees all relevant facts; label coverage and corpus limits

**Validation pair:** conforming - Authorized current record retrieved; violating - another tenant's private chunk enters context.

### AIP-03 - Structured output and tool argument contracts

**Priority:** P0 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Model outputs drive parsers, APIs or actions

**Required evidence:** Versioned output/tool schema, captured model outputs and downstream validation traces. Also require the common evidence contract.

**Pass:** Every accepted action consumes validated schema-conforming arguments within its permission scope.

**Fail:** Invalid output is silently coerced into a forbidden or unintended action.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Valid JSON can still encode incorrect intent; semantic criteria need separate checks

**Validation pair:** conforming - Invalid arguments rejected visibly; violating - parser fallback executes unintended default action.

### AIP-04 - Uncertainty and unsupported-answer behavior

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Prompt, model or answer policy changes

**Required evidence:** Answerable/unanswerable labeled set, error/abstention counts and accepted utility policy. Also require the common evidence contract.

**Pass:** Measured error and abstention tradeoffs meet the accepted criteria on the held-out set.

**Fail:** Unsupported confident answers or excessive refusal breach the accepted evaluation policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Self-reported confidence is not automatically calibrated; do not reward blanket refusal

**Validation pair:** conforming - Missing-evidence question gets explicit uncertainty; violating - invented detail is confidently asserted.

### AIP-05 - Evaluation leakage and train-test separation

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** M · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Training, fine-tuning, prompt tuning or benchmark data changes

**Required evidence:** Dataset lineage, split keys, overlap audit and access records. Also require the common evidence contract.

**Pass:** Accepted separation policy holds across training, tuning and held-out evaluation.

**Fail:** Confirmed prohibited overlap or evaluator exposure violates the separation policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Semantic leakage can evade exact matching; unobservable provider training data remains unknown

**Validation pair:** conforming - Entity/time-separated holdout preserved; violating - answer key copied into tuning context.

### AIP-06 - Quality regression and distribution shift

**Priority:** P1 · **Method:** controlled measurement · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** Model/provider/prompt changes or production input mix drifts

**Required evidence:** Frozen evaluation set, representative slices, version fingerprints and repeated results. Also require the common evidence contract.

**Pass:** Measured quality stays within accepted per-slice and aggregate bounds.

**Fail:** A reproducible important slice or overall result breaches its accepted bound.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: A fixed benchmark can become stale; refresh through an independently reviewed process

**Validation pair:** conforming - New version preserves minority-language slice; violating - average improves while that slice collapses.

### AIP-07 - Harm, bias and privacy review for intended use

**Priority:** P1 · **Method:** human adjudicated review · **Cost:** L · **Gate:** advisory until human decision

**Applies / trigger:** An AI feature enters a consequential or sensitive use case

**Required evidence:** Use-case risk assessment, consented evaluation slices, harmful-output probes and qualified review. Also require the common evidence contract.

**Pass:** Authorized reviewers accept the declared risk criteria and remaining limitations.

**Fail:** Review identifies a material violation of the accepted use-case policy.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Generic software checks do not certify medical, legal, financial or physical safety

**Validation pair:** conforming - Required reviewer confirms bounded use case; violating - release skips a mandatory domain safety evaluation.

### AIP-08 - Inference budgets, fallback and provider failure

**Priority:** P1 · **Method:** deterministic oracle · **Cost:** L · **Gate:** eligible only when accepted as mandatory

**Applies / trigger:** AI request orchestration, streaming or routing changes

**Required evidence:** Token/time/spend policy, provider-failure injection and fallback/cancellation traces. Also require the common evidence contract.

**Pass:** Requests honor accepted cost/deadline limits and truthful fallback behavior.

**Fail:** Retry loops exceed budget or fallback silently changes required semantics.

**Unknown / limits:** Required evidence, a required accepted contract, or adequate authority/coverage is missing or uninterpretable. Additional boundary: Provider-reported usage may arrive late; distinguish estimates from billed measurements

**Validation pair:** conforming - Failure triggers bounded visible fallback; violating - recursive model retries exhaust the accepted budget.

## Relationship to supplied ThruLine material

Documents and synthetic fixture source only; no live repository audit or execution performed.

The files describe the ledger, deterministic derivation, abstention, and transcript-level claim matching. The latest supplied script defines 17 planted scenarios. Its source is not evidence that the current application passes them. Cases 15 and 16 demonstrate an authority gap for a broader product verifier: an agent notification or success-sounding script output needs separate artifact evidence before implementation is called verified.

## Sources

Source IDs identify research context or official adapter documentation. Individual check formulations, thresholds policy and architecture are original proposals; sources do not establish that ThruLine implements them.

[1] ThruLine P0 Build Brief (supplied attachment). Supplied project document; historical status dated 2026-07-29. Product specification and recorded historical measurements; not a current repository audit.

[2] ThruLine North Star (supplied attachment). Supplied project document. Product intent, precision gate, local-first and 8 GB constraints.

[3] ThruLine CLAUDE.md (supplied attachment). Supplied project document. Engineering invariants and required development gates.

[4] ThruLine in-vivo fixtures, M3.11 through M3.15 (supplied attachment). Four supplied shell scripts; v4 is the latest supplied fixture. Synthetic scenario definitions and acceptance logic; scripts were inspected, not executed. Also inspected 04-m311-in-vivo.sh, 05-m314-in-vivo-v3.sh and 06-m313-in-vivo-v2.sh.

[5] [Characterizing the Quality Profile of AI-Generated C++ in Production](https://arxiv.org/html/2608.06640v1). Tran et al.; arXiv v1, 2026-08-06. Large observational study in one enterprise; category differences are not universal causal effects.

[6] [AI-Generated Smells: An Analysis of Code and Architecture in LLM- and Agent-Driven Development](https://arxiv.org/html/2605.02741v1). Zhu, Tsantalis and Rigby; arXiv v1, 2026-05-04. Preprint; code-smell proxies, limited tasks and an architecture experiment using MetaGPT. Raw smell counts correlate with size.

[7] [The Illusion of Safety: Multi-Tier Verification of AI vs. Human C++ Code](https://arxiv.org/pdf/2607.00107). Mahmud et al.; arXiv v3, 2026-08-02. Preprint; competitive-programming tasks and three older open models, not present-day agent incidence.

[8] [We Have a Package for You! A Comprehensive Analysis of Package Hallucinations by Code Generating LLMs](https://www.usenix.org/conference/usenixsecurity25/presentation/spracklen). Spracklen et al.; USENIX Security 2025. Peer-reviewed mechanism evidence from older model cohorts; not a 2026 prevalence estimate.

[9] [The Range Shrinks, the Threat Remains: Re-evaluating LLM Package Hallucinations on the 2026 Frontier-Model Cohort](https://arxiv.org/pdf/2605.17062). Churilov; arXiv, first submitted 2026-05-16. Exploratory preprint. Reused registry snapshots, inconsistent denominator descriptions and refusal differences limit rate comparisons.

[10] [AI builds, We Analyze: An Empirical Study of AI-Generated Build Code Quality](https://arxiv.org/html/2601.16839v1). Ghammam and Almukhtar; arXiv v1, January 2026. 387 selected pull requests and 945 build files; static smells, not measured production incident frequency.

[11] [Stack Overflow 2025 Developer Survey: AI](https://survey.stackoverflow.co/2025/ai). Stack Overflow; 2025 survey. Self-selected respondents; frustration question has 31,476 responses. This is 2025 evidence.

[12] [Announcing the 2025 DORA Report](https://cloud.google.com/blog/products/ai-machine-learning/announcing-the-2025-dora-report). Google Cloud / DORA; 2025-09-23. Observational research summary; associations do not establish causation.

[13] [We are Changing our Developer Productivity Experiment Design](https://metr.org/blog/2026-02-24-uplift-update/). METR; 2026-02-24. Explains why later productivity estimates are affected by selection and concurrent-agent measurement problems.

[14] [April 23 postmortem](https://www.anthropic.com/engineering/april-23-postmortem). Anthropic; 2026-04-23. Vendor incident account of three harness regressions, reported fixed by April 20; not a current unresolved defect.

[15] [Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents). Anthropic; 2025-11-26. Engineering observations and a bounded application-building setup, not a population failure rate.

[16] [Harness design for long-running apps](https://www.anthropic.com/engineering/harness-design-long-running-apps). Anthropic; 2026-03-24. Engineering experience with app-building agents, evaluator calibration and context continuity.

[17] [Demystifying evals for AI agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents). Anthropic; 2026-01-09. Practical evaluation methodology; graders and environment outcomes must be distinguished.

[18] [Brief independent investigation of agents’ behavior, reasoning and collaboration in the OpenAI / Hugging Face hacking incident](https://metr.org/blog/2026-08-26-openai-hugging-face-incident-investigation/). METR; 2026-08-26. Specialized cyber-research incident with evaluation manipulation and transcript spoofing. Does not estimate ordinary coding-agent frequency.

[19] [Agent Control Standard](https://genai.owasp.org/resource/agent-control-standard-acs/). OWASP GenAI project; resource published 2026-09-01; public preview. Emerging interoperability and runtime-control work; availability is not a universal security guarantee.

[20] [Agent Control Standard repository](https://github.com/GenAI-Security-Project/agent-control-standard). OWASP GenAI Security Project; inspected 2026-09-08. Readme describes inspectability, tracing, instrumentation and a public-preview roadmap; implementation maturity varies.

[21] [Application Security Verification Standard](https://owasp.org/www-project-application-security-verification-standard/). OWASP; stable 5.0.0 released 2025-05-30. Versioned security requirements. This report does not claim full ASVS coverage or certification.

[22] [SLSA build provenance](https://slsa.dev/spec/v1.2/build-provenance). SLSA specification v1.2. Provenance structure and trust boundaries; provenance authenticates build origin, not software correctness.

[23] [Version constraint ignored: requested testing-tool migration](https://github.com/anthropics/claude-code/issues/92914). Claude Code issue #92914; opened 2026-09-08. First-person, unconfirmed issue report: testing against 4.0.0 despite a requested 5.2 migration.

[24] [Concurrent configuration writes and worktree state loss](https://github.com/anthropics/claude-code/issues/92908). Claude Code issue #92908; opened 2026-09-08. First-person, unconfirmed report and author diagnosis of configuration locking/state loss on Windows.

[25] [Demo environment and skipped trust/hooks behavior](https://github.com/anthropics/claude-code/issues/92911). Claude Code issue #92911; opened 2026-09-08. First-person, unconfirmed report that an environment flag bypasses expected hooks and workspace setup.

[26] [Scheduled tasks remain pending before first tool call](https://github.com/anthropics/claude-code/issues/92910). Claude Code issue #92910; opened 2026-09-08. First-person, unconfirmed report of scheduler stalls, including long pending intervals.

[27] [Windows runtime crash on a trivial prompt](https://github.com/anthropics/claude-code/issues/92916). Claude Code issue #92916; opened 2026-09-08. First-person, unconfirmed Bun segmentation-fault report for Claude Code 2.1.263.

[28] [ArchUnit user guide](https://www.archunit.org/userguide/html/000_Index.html). Official documentation; inspected 2026-09-08. Java architecture rules, dependency analysis and limits from incomplete class imports.

[29] [dependency-cruiser](https://github.com/sverweij/dependency-cruiser). Official repository; inspected 2026-09-08. JavaScript/TypeScript dependency rules and graph analysis; configure resolvers and entry points.

[30] [AddressSanitizer](https://clang.llvm.org/docs/AddressSanitizer.html). LLVM / Clang official documentation. Instrumented memory-error detection; a clean exercised run does not cover unexecuted paths.

[31] [ThreadSanitizer](https://clang.llvm.org/docs/ThreadSanitizer.html). LLVM / Clang official documentation. Data-race detection; documentation reports substantial typical time and memory overhead.

[32] [Mutant states and metrics](https://stryker-mutator.io/docs/mutation-testing-elements/mutant-states-and-metrics/). Stryker official documentation. Mutation outcomes and metric semantics; equivalent mutants and infrastructure failures need interpretation.

[33] [Trace viewer](https://playwright.dev/docs/trace-viewer). Playwright official documentation. Browser action, DOM and network trace inspection; assertions still define whether a workflow is correct.

[34] [OpenTelemetry semantic conventions](https://opentelemetry.io/docs/specs/semconv/). OpenTelemetry official specification. Versioned conventions for telemetry correlation, not automatic correctness checks.

[35] [Using EXPLAIN](https://www.postgresql.org/docs/current/using-explain.html). PostgreSQL official current documentation; inspected 2026-09-08. Query plan and actual execution statistics. EXPLAIN ANALYZE executes the statement.

[36] [Write-Ahead Logging](https://www.sqlite.org/wal.html). SQLite official documentation; inspected 2026-09-08. Single writer, checkpointing, durability and same-host limitations matter for a local ledger.

[37] [Proptest](https://docs.rs/proptest/latest/proptest/). Official Rust crate documentation. Property-based generation and shrinking; quality depends on the chosen properties and generators.

[38] [Loom](https://docs.rs/loom/latest/loom/). Official Rust crate documentation. Bounded concurrent-execution exploration; only instrumented synchronization is visible.

[39] [OSS-Fuzz](https://google.github.io/oss-fuzz/). Google official project documentation. Continuous fuzzing infrastructure; a fuzz campaign remains bounded by its harness and resources.

[40] [Knip](https://knip.dev/). Official project documentation. Unused files, exports and dependency analysis; dynamic entry points require configuration and review.

[41] [Web Content Accessibility Guidelines](https://www.w3.org/WAI/standards-guidelines/wcag/). W3C official guidance. Accessibility standards; automated scanning alone does not establish complete accessibility.

[42] [Open Source Vulnerabilities](https://osv.dev/). OSV official project. Version/commit-linked known-vulnerability data; database freshness and package matching affect coverage.

[43] [Why language models hallucinate](https://openai.com/index/why-language-models-hallucinate/). OpenAI research explanation; 2025-09-05. Argues that training and accuracy-only evaluation can reward guessing over uncertainty; not a universal empirical hallucination rate.

[44] [Artificial Intelligence Risk Management Framework: Generative Artificial Intelligence Profile](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.600-1.pdf). NIST AI 600-1; July 2024. Foundational voluntary cross-sector risk profile, used for breadth rather than as current legal guidance or certification.

