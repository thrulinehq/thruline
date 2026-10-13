/* VS Code's documented file URL opens an installed editor through the OS.
 * This is a link from a recorded local path, with no ThruLine action route. */
export function editorHref(path, line = null) {
  if (typeof path !== 'string' || !path.startsWith('/') || path.startsWith('//')
    || /[\\\0\r\n]/.test(path) || path.split('/').some((part) => part === '.' || part === '..')
    || (line != null && (!Number.isInteger(line) || line < 1))) return null
  const encoded = path.split('/').map(encodeURIComponent).join('/')
  return `vscode://file${encoded}${line == null ? '' : `:${line}:1`}`
}
