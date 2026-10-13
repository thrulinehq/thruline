#!/usr/bin/env node
// Hands off to the downloaded binary, preserving its exit code — ThruLine's
// exit codes are its CI interface (0 ok, 1 error, 3 policy failure), and a
// wrapper that swallowed one would turn a failing gate green.
'use strict';
const path = require('path');
const { spawnSync } = require('child_process');
const bin = path.join(__dirname, 'thruline-bin');
const r = spawnSync(bin, process.argv.slice(2), { stdio: 'inherit' });
if (r.error) {
  console.error(`thruline: ${r.error.message}`);
  process.exit(1);
}
process.exit(r.status === null ? 1 : r.status);
