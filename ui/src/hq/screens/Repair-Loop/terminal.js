/* The command "Or in a terminal, through ThruLine" copies (C30 P0-4).
 *
 * `thruline check` has `--for-change`, not `--scope`: the run is recorded
 * under the Change, and its scope is the command's own arguments. So the
 * command is `thruline check --for-change <id> -- <runner> <scope…>`.
 *
 * Two honest variations, and nothing else:
 * - A recorded command that a shell would split — `cd x && pytest`, a pipe,
 *   a redirection, a leading `NAME=value` — is handed to `--shell` as one
 *   single-quoted word. Pasted after a bare `--`, the person's own shell would
 *   run everything after the first `&&` outside the record.
 * - Work not tracked as a Change has no id to name, so its command is the
 *   one `thruline check` records and attaches to nothing.
 *
 * terminal-command.json pins examples of every form; this module must copy
 * each exactly (repair-loop-run.test.js), and
 * crates/cli/tests/repair_loop_terminal_command.rs RUNS each one, as copied,
 * on a scratch repository and a scratch store. PURE.
 */

/** A shell would cut the line here, so the whole command goes to `--shell`. */
const SPLITS = /[|&;<>()`\n]|\$\(|^\s*[A-Za-z_][A-Za-z0-9_]*=/

/** One word a POSIX shell reads back exactly: single quotes, with ' as '\''. */
export const quoted = (s) => "'" + String(s).replace(/'/g, "'\\''") + "'"

/**
 * @param {{changeId?: string|null, runner?: string|null, scope?: string[]}} what
 * @returns {string|null} the line to paste, or null when there is no command to run
 */
export function terminalCommand({ changeId, runner, scope } = {}) {
  if (!runner) return null
  const command = [runner, ...(Array.isArray(scope) ? scope : [])].join(' ')
  const shell = SPLITS.test(command)
  return ['thruline check']
    .concat(shell ? ['--shell'] : [])
    .concat(changeId ? ['--for-change ' + changeId] : [])
    .concat(['--', shell ? quoted(command) : command])
    .join(' ')
}

export default terminalCommand
