'use strict';
// Rune Agent Booster sidecar (ADR 003). Reads one JSON request on stdin
// ({code, edit, language}) and writes one JSON result on stdout. The edited
// text is `result.output` (not `result.code`). Library logging goes to stderr
// so stdout carries exactly one JSON line.
const path = require('node:path');
console.log = console.error;
console.info = console.error;
let data = '';
process.stdin.setEncoding('utf8');
process.stdin.on('data', (c) => { data += c; });
process.stdin.on('end', async () => {
  let out;
  try {
    const req = JSON.parse(data);
    const { AgentBooster } = require(path.join(process.env.RUNE_NODE_MODULES, 'agent-booster'));
    const r = await new AgentBooster().apply({
      code: req.code,
      edit: req.edit,
      language: req.language,
    });
    out = {
      success: r.success === true,
      output: typeof r.output === 'string' ? r.output : null,
      confidence: typeof r.confidence === 'number' ? r.confidence : null,
    };
  } catch (_) {
    out = { success: false, output: null, confidence: null };
  }
  process.stdout.write(JSON.stringify(out) + '\n');
});
