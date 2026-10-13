/* "Was the bar accepted after the agent's last edit?" is an ORDER OF TWO MOMENTS.
 * HH:MM faces cannot say it across midnight ("00:10" sorts before "23:50"), so when the
 * record holds both instants they decide; the faces decide only when it holds neither
 * (the design gallery). PURE. */
export function acceptedAfterWork(acceptedTs, lastEditTs, acceptedFace, lastEditFace) {
  const a = Date.parse(acceptedTs)
  const e = Date.parse(lastEditTs)
  return Number.isFinite(a) && Number.isFinite(e) ? a > e : acceptedFace > lastEditFace
}
