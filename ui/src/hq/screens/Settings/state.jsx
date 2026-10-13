/* Settings while the record is being read, and when it cannot be: said inside the
 * app frame (frame-state.jsx), never as a page of its own. */
import React from 'react'
import { Reading, Failed as InFrame } from '../../frame-state.jsx'

export { Reading }

export function Failed() {
  return <InFrame what="ThruLine on this machine did not answer, so Settings can’t show anything yet. Start it with thruline ui, then reload this page." />
}
