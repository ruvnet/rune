"""Audit source/report binding independently of the study selection driver."""
import hashlib
import json
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parent

def sha(b):
    return hashlib.sha256(b).hexdigest()

def tree(path):
    h = hashlib.sha256()
    for p in sorted(path.rglob('*')):
        if not p.is_file() or p.is_symlink() or any(x in p.relative_to(path).parts for x in ['target','.git','.rune','node_modules']):
            continue
        rel = str(p.relative_to(path)).encode()
        content = p.read_bytes()
        h.update(struct.pack('<Q',len(rel))); h.update(rel)
        h.update(struct.pack('<Q',len(content))); h.update(content)
    return h.hexdigest()

manifest = json.loads((ROOT/'manifest.json').read_text())
checked = 0
for w in manifest:
    base = ROOT/'workloads'/w['name']
    contract_file = (base/'contract.json').read_bytes()
    contract = json.loads(contract_file)
    contract_digest = sha(json.dumps(contract,separators=(',',':')).encode())
    for s in ['counting','radix','unstable']:
        r = json.loads((ROOT/'reports'/w['name']/(s+'.json')).read_text())
        assert r['baseline_digest'] == tree(base/'baseline'), w['name']
        assert r['candidate_digest'] == tree(base/s), (w['name'],s)
        assert r['contract_digest'] == contract_digest, (w['name'],s)
        assert r['evidence']['contract_file_sha256'] == sha(contract_file)
        assert r['evidence']['holdout_tree_sha256'] == tree(base/'holdout')
        assert r['evidence']['evaluator_exe_sha256'] == sha((ROOT.parent/'target/debug/rune-eval').read_bytes())
        if r['verdict']['status'] == 'accept':
            assert r['samples'] == 40 and r['speedup_ci95_low'] > 1.10
            assert len(r['evidence']['metrics']) == 2
            assert all(m['passed'] for m in r['evidence']['metrics'])
        checked += 1
out = dict(reports_checked=checked,all_source_contract_holdout_and_evaluator_digests_match=True,
           protocol_sha256=sha((ROOT/'PREREGISTRATION.md').read_bytes()),
           selection_sha256=sha((ROOT/'selection.json').read_bytes()))
(ROOT/'audit.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
