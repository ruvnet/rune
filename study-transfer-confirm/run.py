"""Fixed-size prospective confirmation. No new training or memory updates."""
import concurrent.futures as cf
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT=Path(__file__).resolve().parent
OLD=ROOT.parent/'study-transfer'
spec=importlib.util.spec_from_file_location('pilot',OLD/'run.py')
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.ROOT=ROOT

def generate():
    original=json.loads((OLD/'manifest.json').read_text())
    library=json.loads((OLD/'selection.json').read_text())
    manifest=[]
    for old in original:
        if old['training'] or old['seed']!=2718:continue
        for i in range(24):
            seed=100003+104729*i
            w=dict(old,name=f"confirm_{old['n']}_{old['range']}_{old['order']}_{seed}",seed=seed)
            base=ROOT/'workloads'/w['name']
            for strategy in ['baseline',*m.STRATEGIES]:
                src=OLD/'workloads'/old['name']/strategy
                for p in src.rglob('*'):
                    if not p.is_file():continue
                    dest=base/strategy/p.relative_to(src);dest.parent.mkdir(parents=True,exist_ok=True)
                    text=p.read_text()
                    if p.name=='bench.rs':text=text.replace(f',{old["seed"]},',f',{seed},')
                    dest.write_text(text)
            (base/'holdout').mkdir(exist_ok=True)
            shutil.copy(OLD/'workloads'/old['name']/'holdout/holdout.rs',base/'holdout/holdout.rs')
            c=json.loads((OLD/'workloads'/old['name']/'contract.json').read_text())
            c['name']=w['name'];c['bench_cmd']=['./target/release/bench',str(w['n'])]
            c['guard_benches'][0]['bench_cmd']=['./target/release/bench',str(w['n']+7)]
            m.dump(base/'contract.json',c);manifest.append(w)
    request=dict(recipes=library['recipes'],queries=[w['fingerprint'] for w in manifest])
    ranks=json.loads(subprocess.check_output([str(ROOT.parent/'target/debug/rune-rank-study')],input=json.dumps(request),text=True))
    choices={w['name']:r[0] for w,r in zip(manifest,ranks)}
    library=dict(library,choices=choices)
    for path,value in [(ROOT/'manifest.json',manifest),(ROOT/'selection.json',library)]:
        if path.exists():assert json.loads(path.read_text())==value,'frozen input mismatch'
        else:m.dump(path,value)
    m.dump(ROOT/'selection-freeze.json',dict(sha256=m.digest(ROOT/'selection.json')))
    paths=[ROOT/'PREREGISTRATION.md',ROOT/'run.py',ROOT/'manifest.json',ROOT/'selection.json']
    paths+=sorted((ROOT/'workloads').rglob('*'))
    freeze={str(p.relative_to(ROOT)):m.digest(p) for p in paths if p.is_file()}
    if (ROOT/'freeze.json').exists():assert json.loads((ROOT/'freeze.json').read_text())==freeze
    else:m.dump(ROOT/'freeze.json',freeze)
    print('frozen',len(manifest),'fresh cases; memory unchanged')

def evaluate():
    m.verify_freeze()
    ws=json.loads((ROOT/'manifest.json').read_text())
    tasks=[(w,s) for w in ws for s in m.STRATEGIES]
    cpus=sorted(os.sched_getaffinity(0))[:6]
    def queue(k):
        for i in range(k,len(tasks),len(cpus)):
            w,s=tasks[i];out=ROOT/'reports'/w['name']/(s+'.json')
            if out.exists():continue
            out.parent.mkdir(parents=True,exist_ok=True);base=ROOT/'workloads'/w['name']
            cmd=['taskset','-c',str(cpus[k]),str(ROOT.parent/'target/debug/rune-eval'),
                 '--contract',str(base/'contract.json'),'--baseline',str(base/'baseline'),
                 '--candidate',str(base/s),'--workdir',str(ROOT/'scratch'/w['name']/s),
                 '--out',str(out),'--allow-degraded']
            p=subprocess.run(cmd,capture_output=True,text=True,timeout=330)
            if p.returncode not in [0,1] or not out.exists():raise RuntimeError(p.stderr)
            r=json.loads(out.read_text())
            if r['verdict'].get('reject',{}).get('reason') in ['build_failed','timeout','missing_evidence']:
                raise RuntimeError(str(r['verdict']))
            print(w['name'],s,r['verdict']['status'],round(r['speedup_mean'],3),flush=True)
    with cf.ThreadPoolExecutor(max_workers=len(cpus)) as pool:
        for f in [pool.submit(queue,k) for k in range(len(cpus))]:f.result()

def analyze():
    m.analyze()
    path=ROOT/'result.json';r=json.loads(path.read_text());rows=r['rows']
    blocks={}
    for row in rows:blocks.setdefault(row['name'].rsplit('_',1)[1],[]).append(row)
    assert len(rows)==288 and len(blocks)==24 and all(len(v)==12 for v in blocks.values())
    block_ps={a:m.pvalue([sum(x['memory']-x[a] for x in block)/12 for block in blocks.values()]) for a in ['random','fixed']}
    r['seed_block_p_values']=block_ps
    r['other_fixed_rates']={s:sum(x['outcomes'][s] for x in rows)/len(rows) for s in m.STRATEGIES}
    r['frozen_memory_matches_training']=json.loads((ROOT/'selection.json').read_text())['recipes']==json.loads((OLD/'selection.json').read_text())['recipes']
    assert r['frozen_memory_matches_training']
    if not all(r['p_values'][a]<0.0125 and block_ps[a]<0.0125 for a in ['random','fixed']):r['verdict']='NoDifference'
    r['scope']='fixed memory selection on fresh input draws in twelve synthetic sorting contexts; not general self improvement'
    m.dump(path,r)
    print(json.dumps({k:v for k,v in r.items() if k!='rows'},indent=2))

if __name__=='__main__':
    {'generate':generate,'run':evaluate,'analyze':analyze}[sys.argv[1]]()
