export const CAPTURE_OPERATIONAL_STATES = [
  'not_available',
  'bounded_only',
  'disabled',
  'available_not_installed',
  'degraded_source_version',
  'degraded_configuration_drift',
  'degraded_key_or_identity',
  'installed_unverified',
  'degraded_source_pairing',
  'unsupported_profile',
  'capture_error',
  'assured_ready',
]

export const captureKeyboardActivation = (key) => key === 'Enter' || key === ' '

const COPY = {
  not_available: ['Assured Capture unavailable', 'This source has no admitted local assurance contract.'],
  bounded_only: ['Bounded observations only', 'This source can record explicit observations, but cannot prove the command boundary.'],
  disabled: ['Assured Capture disabled', 'No new commands are captured; historical proof remains.'],
  available_not_installed: ['Assured Capture available', 'Review the exact local hook changes before installation.'],
  degraded_source_version: ['Source version not qualified', 'Earlier proof is preserved; new commands will not be assured.'],
  degraded_configuration_drift: ['Capture configuration changed', 'New assurance is paused until a fresh plan is reviewed.'],
  degraded_key_or_identity: ['Collector identity unavailable', 'Existing signatures remain recorded; new assurance is paused.'],
  installed_unverified: ['Installation not verified', 'Run the isolated source-native self-test before authorizing this project.'],
  degraded_source_pairing: ['Source pairing incomplete', 'A complete source result link was not accepted; new assurance is paused.'],
  unsupported_profile: ['Command profile not assured', 'This command is outside the measured local foreground contract.'],
  capture_error: ['Capture needs attention', 'The latest capture did not complete; historical proof remains.'],
  assured_ready: ['Assured Capture ready', 'Supported Codex commands can create execution-boundary proof for this authorized project.'],
}

export function captureProjectView(data) {
  const capture = data.assured_capture
  const source = data.project?.source
  const fallback = source === 'claude_code' ? 'bounded_only' : 'not_available'
  const state = CAPTURE_OPERATIONAL_STATES.includes(capture?.operational_state)
    ? capture.operational_state
    : fallback
  const [title, defaultDetail] = COPY[state]
  const receipts = Object.values(data.capture_receipts || {})
  const counts = { current: 0, stale: 0, unknown: 0 }
  for (const receipt of receipts) {
    const key = ['current', 'stale'].includes(receipt.currentness) ? receipt.currentness : 'unknown'
    counts[key] += 1
  }
  const currency = counts.stale > 0
    ? `${counts.stale} stale proof receipt${counts.stale === 1 ? '' : 's'}`
    : counts.current > 0
      ? `${counts.current} current proof receipt${counts.current === 1 ? '' : 's'}`
      : receipts.length > 0
        ? `${counts.unknown} proof receipt${counts.unknown === 1 ? '' : 's'} with unknown currentness`
        : 'No captured proof'
  return {
    state,
    title,
    detail: capture?.explanation || defaultDetail,
    ready: state === 'assured_ready',
    authorized: capture?.project_authorized === true,
    currency,
    counts,
    accepted: capture?.accepted_capture_count || receipts.length,
  }
}

export function captureReceiptView(receipt) {
  if (!receipt) return null
  const strength = receipt.strength === 'assured' ? 'Assured at execution' : 'Bounded observation'
  const currency = receipt.currentness === 'current'
    ? 'Current as of accepted observation'
    : receipt.currentness === 'stale'
      ? 'Stale as of accepted observation'
      : 'Currentness unknown'
  return {
    strength,
    currency,
    currentness: receipt.currentness || 'unknown',
    capturedAt: receipt.execution?.accepted_at_unix_ms || null,
    currentAsOf: receipt.current_as_of_unix_ms || null,
    reason: (receipt.currentness_reason || 'no_comparable_observation').replaceAll('_', ' '),
    changed: receipt.changed_dimensions || [],
    exit: receipt.execution?.term_signal != null
      ? `Signal ${receipt.execution.term_signal}`
      : receipt.execution?.exit_code != null
        ? `Exit ${receipt.execution.exit_code}`
        : 'Result unavailable',
    scope: receipt.execution?.complete === true ? 'Repository and admitted toolchain scope complete' : 'Execution scope incomplete',
    verification: receipt.execution?.receipt_verified === true && receipt.execution?.capsules_verified === true
      ? 'Receipt and capsules verified'
      : 'Verification incomplete',
    pairing: receipt.execution?.source_pairing === 'complete' ? 'Source result paired' : 'Source pairing incomplete',
    history: (receipt.proof_history || []).map((entry) => ({
      currentness: entry.currentness || 'unknown',
      changed: entry.changed_dimensions || [],
      reason: entry.reason || '',
      observedAt: entry.observed_at_unix_ms || null,
    })),
  }
}

// Binding M5.R2.R0 fixture matrix. Copy stays plain-language and keeps truth,
// capture strength, and currency separate.
export const CAPTURE_UX_SCENARIOS = [
  [1, 'codex_exact_installed_ready', 'Assured Capture ready', 'Supported Codex commands can create execution-boundary proof for this authorized project.', 'Everything is healthy'],
  [2, 'codex_exact_not_installed', 'Assured Capture is available', 'Review the exact local hook changes before installation.', 'Protected'],
  [3, 'codex_upgraded_unqualified', 'Codex version is not qualified', 'Earlier proof is preserved; new commands will not be assured.', 'Probably compatible'],
  [4, 'codex_configuration_drift', 'Capture configuration changed', 'New assurance is paused until you review a fresh plan.', 'Auto-fixed'],
  [5, 'collector_key_missing', 'Collector identity is unavailable', 'Existing signatures remain recorded; new assurance is paused.', 'New key created'],
  [6, 'collector_key_replaced', 'Collector identity changed', "The new key cannot inherit the old key's authority.", 'Key updated successfully'],
  [7, 'last_command_assured_current', 'Verified at execution and current as of observation', 'Proof matches the accepted observation.', 'Current without as-of time'],
  [8, 'last_command_assured_stale', 'Verified at execution; repository or toolchain changed', 'Proof is stale as of the accepted observation.', 'Claim is false'],
  [9, 'historically_assured_no_observation', 'Verified historically; currentness unknown', 'No comparable later observation has been accepted.', 'Still verified'],
  [10, 'pre_missing', 'Before-state missing', 'Capture started too late to prove the execution boundary.', 'Command failed'],
  [11, 'post_missing', 'After-state missing', 'The command may have run; this attempt is not assured.', 'Command did not run'],
  [12, 'source_callback_missing', 'Source result could not be linked', 'The source did not deliver a complete result link.', 'Result linked'],
  [13, 'unsupported_command_profile', 'This command profile is not assured', 'It is outside the measured local foreground contract.', 'Partially assured'],
  [14, 'repository_failed_stabilization', 'Repository state kept changing', 'No mixed state was accepted.', 'Latest state captured'],
  [15, 'non_git_repository', 'Assured repository capture is unavailable here', 'This project lacks the required Git scope.', 'Current'],
  [16, 'linked_worktree', 'Linked worktree recognized', 'Assurance is available when the exact profile and stabilization checks pass.', 'Same as main worktree'],
  [17, 'orphan_recoverable', 'An unfinished capture record can be recovered', 'Recovery records what is knowable and never reruns the command.', 'Retry command'],
  [18, 'uninstall_requested', 'Review uninstall plan', 'Only verified ThruLine hook entries will be removed; proof history stays.', 'Delete Assured Capture data'],
  [19, 'disabled_history_retained', 'Capture is disabled', 'No new commands are captured; historical proof remains.', 'Proof disabled'],
  [20, 'claude_bounded_only', 'Claude supports bounded observations only', 'Its execution timing boundary has not been measured locally.', 'Claude capture ready'],
  [21, 'source_unsupported', 'Assured Capture is not available for this source', 'This source lacks an admitted local contract.', 'Unhealthy'],
  [22, 'no_proof', 'No captured proof exists', 'Truth may still have other evidence; capture status is separate.', 'Unsupported'],
].map(([id, scenario, title, body, forbidden]) => ({ id, scenario, title, body, forbidden }))
