import React from 'react'

export function SemgrepModeFacts({ value }) {
  const current = value.current
  const delta = value.delta
  const tests = value.rule_test
  return <section aria-label="Selected Semgrep mode">
    <h5>Selected Semgrep mode: {value.mode || 'unavailable'}</h5>
    <p>These local modes use the same Semgrep CE engine. They do not add cross-function or cross-file coverage.</p>
    {value.capture_error ? <p role="alert">Native capture incomplete: {value.capture_error}</p> : null}
    {current ? <>
      <p>Current source findings: {current.results?.length ?? 'unavailable'}. Scanned files: {current.paths?.scanned?.length ?? 'unavailable'}.</p>
      {current.paths?.skipped?.length ? <div role="alert"><p>Ignored or excluded source is outside this completed scope.</p><ul>{current.paths.skipped.map((file, index) => <li key={index}>{file.path} · {file.reason}</li>)}</ul></div> : <p>The native targeting inventory reports no skipped paths.</p>}
    </> : null}
    {value.preparation ? <details><summary>Bounded source-snapshot preparation</summary><p>The current candidate prepared this source tree in a separate worker. Network was denied; source, real index, refs and configuration remained read-only. Only Git objects and private scratch were writable.</p><pre>{JSON.stringify(value.preparation, null, 2)}</pre></details> : null}
    {value.mode === 'baseline' ? <>
      <p>Accepted ancestor: {value.git_identity?.baseline || 'unavailable'}. Current commit: {value.git_identity?.head || 'unavailable'}.</p>
      <p>New findings relative to ancestor: {delta?.results?.length ?? 'unavailable'}. A zero-new-findings result can retain older current findings; it does not establish a clean project.</p>
      <details><summary>Bound baseline object and tree identity</summary><pre>{JSON.stringify(value.git_identity, null, 2)}</pre></details>
      <details><summary>All current native findings, including older findings</summary><pre>{JSON.stringify(current?.results, null, 2)}</pre></details>
    </> : null}
    {tests ? <>
      <p>Native rule assertions: {tests.checks?.length || 0}. Native fixed-fixture assertions: {Object.keys(tests.native?.fixtest_results || {}).length}.</p>
      <p>These assertions compare selected matches and native fix text. They do not verify application behavior or apply a source patch.</p>
      {tests.checks?.map((check, index) => <article key={index}><p>{check.rule} · {check.fixture} · native assertion {check.assertion.passed ? 'passed' : 'failed'}.</p><pre>{JSON.stringify(check.assertion.matches, null, 2)}</pre>{check.assertion.errors?.length ? <pre>{JSON.stringify(check.assertion.errors, null, 2)}</pre> : null}</article>)}
      <details open><summary>Native fix assertions and missing fixture coverage</summary><pre>{JSON.stringify({ fixes: tests.native?.fixtest_results, missingTests: tests.native?.config_missing_tests, missingFixes: tests.native?.config_missing_fixtests, configurationErrors: tests.native?.config_with_errors, todoAnnotations: tests.inventory?.todo_annotations, sourceAnnotations: tests.inventory?.rules }, null, 2)}</pre></details>
    </> : null}
    <details><summary>Explicit local mode configuration</summary><pre>{JSON.stringify(value.selection, null, 2)}</pre></details>
  </section>
}
