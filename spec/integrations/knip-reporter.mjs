// Knip 6.35.0 ReporterOptions -> native issue groups and completion-bearing scope.
// This reporter transforms native values only; it does not scan or execute files.
import { isAbsolute, relative } from 'node:path';
const location = (cwd, value) => typeof value === 'string' && isAbsolute(value) ? relative(cwd, value) : value;
const symbol = item => ({ name: item.symbol ?? '', namespace: item.parentSymbol,
  kind: item.kind, specifier: item.specifier, line: item.line, col: item.col, pos: item.pos });
export default function reportKnip({ report, issues, counters, configurationHints,
  tagHints, enabledPlugins, includedWorkspaceDirs, selectedWorkspaces, cwd,
  isProduction, configFilePath }) {
  const rows = new Map(), nativeIssues = [];
  for (const [category, enabled] of Object.entries(report)) {
    if (!enabled) continue;
    for (const item of Object.values(issues[category]).flatMap(Object.values)) {
      const file = relative(cwd, item.filePath);
      if (!rows.has(file)) rows.set(file, { file });
      const row = rows.get(file);
      const group = category === 'duplicates' || category === 'cycles';
      if (group && (!Array.isArray(item.symbols) || !item.symbols.length)) throw Error('Native Knip group has no symbols');
      (row[category] ??= []).push(group ? item.symbols.map(symbol) : symbol(item));
      nativeIssues.push({ category, file, workspace: location(cwd, item.workspace),
        ...symbol(item), symbolType: item.symbolType, severity: item.severity,
        isFixed: item.isFixed ?? false, symbols: item.symbols?.map(symbol) });
    }
  }
  const result = { issues: [...rows.values()], nativeIssues, coverage: {
    schema: 'thruline.knip-coverage.v1', counters, report, configurationHints,
    tagHints: [...(tagHints ?? [])], enabledPlugins, includedWorkspaceDirs,
    selectedWorkspaces: selectedWorkspaces ?? null,
    nativeOptions: { isProduction: !!isProduction,
      configFilePath: location(cwd, configFilePath) ?? null },
  }};
  process.stdout.write(JSON.stringify(result, (_key, value) => value instanceof RegExp ? value.toString() : value) + '\n');
}
