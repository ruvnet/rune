const assert = require('node:assert/strict');
const {AgentBooster} = require('agent-booster');
(async () => {
  const code = 'pub fn add(a: i32, b: i32) -> i32 { a + b }';
  const edit = 'pub fn sum(a: i32, b: i32) -> i32 { a + b }';
  const result = await new AgentBooster().apply({code, edit, language: 'rust'});
  assert.equal(result.success, true);
  assert.equal(result.output, edit);
  console.log(JSON.stringify({package: 'agent-booster@0.2.2', fixture_passed: true,
    latency_ms_reported: result.latency, confidence: result.confidence,
    strategy: result.strategy, semantic_validation: 'not performed'}));
})().catch(() => { console.error('Agent Booster fixture failed'); process.exit(1); });
