import React from 'react'

export function MiniHarnessFacts({ value }) {
  const native = value.native
  return <section aria-label="Native mini-SWE compatibility harness">
    <h5>Owned compatibility task</h5>
    <p>mini-SWE-agent runs the selected owned task through its native SWE-bench runner. This run has no official benchmark score.</p>
    <dl><dt>Task identifier</dt><dd>{value.options?.instance_id || 'Unavailable'}</dd><dt>Environment selection</dt><dd>{value.options?.environment || 'Unavailable'}</dd><dt>Native environment class</dt><dd>{native?.environment_type || 'Native capture unavailable'}</dd><dt>Native agent</dt><dd>{native?.native_agent || 'Unavailable'}</dd><dt>Native termination</dt><dd>{native?.native_exit_status || 'Unknown'}</dd></dl>
    <p>LocalEnvironment executes inside the product-owned Linux namespace with the fixed image, selected workspace and network disabled. Upstream DockerEnvironment, Singularity, SWE-ReX, Bubblewrap and remote environments require separate configured paths.</p>
    <p>Predictions contain the model submission. A submission, a step limit and independent behavior checks remain separate outcomes. Official SWE-bench or ProgramBench task evaluation is outside this capture.</p>
    {value.capture_error ? <p role="status">{value.capture_error}</p> : null}
    {native ? <details><summary>Inspect native harness facts</summary><pre>{JSON.stringify(native, null, 2)}</pre></details> : null}
    {value.source_sha256 ? <p>Native metadata SHA256: <code>{value.source_sha256}</code></p> : null}
  </section>
}
