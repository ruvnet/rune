"""Prospective single-family transfer study. See PREREGISTRATION.md."""
import concurrent.futures as cf
import hashlib
import itertools
import json
import os
from pathlib import Path
import random
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parent
STRATEGIES = ['counting', 'radix', 'unstable']

def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + '\n')

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def fp(w):
    return dict(source_tags=['sort:u16', 'size:' + ('small' if w['n'] < 256 else 'medium' if w['n'] < 4096 else 'large')],
                target_triple='x86_64-unknown-linux-gnu', cpu_features=['study-host'],
                compiler_id=subprocess.check_output(['rustc', '-vV'], text=True), abi='sysv64',
                workload_tags=['range:' + ('narrow' if w['range'] <= 256 else 'wide'), 'order:' + w['order']],
                policy_digest='transfer-study-v2')

LIBS = {
 'baseline': 'pub fn sort(a: &[u16]) -> Vec<u16> { let mut v = a.to_vec(); v.sort(); v }',
 'unstable': 'pub fn sort(a: &[u16]) -> Vec<u16> { let mut v = a.to_vec(); v.sort_unstable(); v }',
 'counting': '''pub fn sort(a: &[u16]) -> Vec<u16> {
    if a.is_empty() { return vec![]; }
    let lo = *a.iter().min().unwrap(); let hi = *a.iter().max().unwrap();
    let mut counts = vec![0usize; (hi as usize) - (lo as usize) + 1];
    for &x in a { counts[(x-lo) as usize] += 1; }
    let mut out = Vec::with_capacity(a.len());
    for (i, &n) in counts.iter().enumerate() { for _ in 0..n { out.push(lo + i as u16); } }
    out
}''',
 'radix': '''pub fn sort(a: &[u16]) -> Vec<u16> {
    let mut v = a.to_vec(); let mut tmp = vec![0u16; a.len()];
    for shift in [0u32, 8] {
        let mut counts = [0usize;256];
        for &x in &v { counts[((x >> shift) & 255) as usize] += 1; }
        let mut pos = [0usize;256]; for i in 1..256 { pos[i] = pos[i-1] + counts[i-1]; }
        for &x in &v { let b = ((x >> shift) & 255) as usize; tmp[pos[b]] = x; pos[b] += 1; }
        std::mem::swap(&mut v, &mut tmp);
    }
    v
}'''
}
GEN = '''fn gen(n: usize, range: u32, mut s: u64, near: bool) -> Vec<u16> {
 let mut a: Vec<u16> = (0..n).map(|_| { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s % range as u64) as u16 }).collect();
 if near { a.sort_unstable(); if n > 3 { a.swap(n/3, n/3+1); } } a
}'''
TEST = GEN + '''
#[test] fn agrees() {
 for n in [0,1,3,7,31,129,1025,4099] { for range in [1,64,65536] { for seed in [11,19,43] {
 let a = gen(n,range,seed,false); let mut expected=a.clone(); expected.sort();
 assert_eq!(kernel::sort(&a), expected); } } }
}
'''
BENCH = '''use std::{hint::black_box, time::Instant};
@GEN@
fn main() {
 let n = std::env::args().nth(1).unwrap().parse::<usize>().unwrap();
 let a = gen(n,@RANGE@,@SEED@,@NEAR@);
 let batch = |iters: u64| { let t=Instant::now(); for _ in 0..iters { black_box(kernel::sort(black_box(&a))); }
 t.elapsed().as_nanos() as f64 / iters as f64 };
 let mut iters=1u64; while iters < (1<<24) && batch(iters)* (iters as f64) < 200000.0 { iters*=2; }
 let mut xs: Vec<f64> = (0..5).map(|_|batch(iters)).collect(); xs.sort_by(f64::total_cmp);
 println!("{{\\\"ns_per_op\\\":{:.4}}}",xs[2]);
}
'''

def generate():
    if (ROOT/'manifest.json').exists():
        raise SystemExit('manifest already frozen; refusing to regenerate')
    workloads = []
    for train, ns, ranges, seeds in [(True,[64,1024,8192],[32,65536],[1701]),
                                    (False,[96,1536,12288],[64,32768],[2718,3141,5772])]:
        for n,r,o,s in itertools.product(ns,ranges,['random','near'],seeds):
            w = dict(name=f'{"train" if train else "test"}_{n}_{r}_{o}_{s}',training=train,n=n,range=r,order=o,seed=s)
            w['fingerprint'] = fp(w)
            bench = BENCH.replace('@GEN@',GEN).replace('@RANGE@',str(r)).replace('@SEED@',str(s)).replace('@NEAR@',str(o=='near').lower())
            base = ROOT/'workloads'/w['name']
            for strategy, lib in LIBS.items():
                d = base/strategy
                for p, content in {'Cargo.toml':'[package]\nname="kernel"\nversion="0.1.0"\nedition="2021"\n[workspace]\n',
                                   'src/lib.rs':lib+'\n','src/bin/bench.rs':bench,
                                   'tests/visible.rs':'#[test] fn visible(){ assert_eq!(kernel::sort(&[3,1,2]),vec![1,2,3]); }\n'}.items():
                    path=d/p; path.parent.mkdir(parents=True,exist_ok=True); path.write_text(content)
            (base/'holdout').mkdir(); (base/'holdout'/'holdout.rs').write_text(TEST)
            cmd=lambda *a:['cargo',*a,'--release','--offline','--quiet']
            benchcmd=lambda n:cmd('run')+['--bin','bench','--',str(n)]
            contract=dict(name=w['name'],kernel_dir='.',test_cmd=cmd('test')+['--test','visible'],
                          holdout_dir='holdout',holdout_dest='tests',holdout_cmd=cmd('test')+['--test','holdout'],
                          bench_cmd=benchcmd(n),guard_benches=[dict(name='n_plus_7',bench_cmd=benchcmd(n+7),tolerance=0.1)],
                          samples=40,warmup=5,min_improvement=0.1,forbid_unsafe=True,forbid_new_deps=True,
                          forbid_new_capabilities=True,max_seconds=300,max_memory_mb=4096)
            dump(base/'contract.json',contract)
            workloads.append(w)
    dump(ROOT/'manifest.json',workloads)
    freeze = {str(p.relative_to(ROOT)):digest(p) for p in ROOT.rglob('*') if p.is_file() and 'target' not in p.parts}
    dump(ROOT/'freeze.json',freeze)
    print('frozen',len(workloads),'contexts')

def verify_freeze():
    for name, h in json.loads((ROOT/'freeze.json').read_text()).items():
        assert digest(ROOT/name)==h, name

def evaluate(split):
    verify_freeze()
    if split=='test':
        assert (ROOT/'selection.json').exists(), 'freeze training memory and choices first'
    manifest=json.loads((ROOT/'manifest.json').read_text())
    tasks=[(w,s) for w in manifest if w['training']==(split=='train') for s in STRATEGIES]
    cpus=sorted(os.sched_getaffinity(0))[:3]
    def worker(item):
        i,(w,s)=item
        out=ROOT/'reports'/w['name']/(s+'.json'); out.parent.mkdir(parents=True,exist_ok=True)
        if out.exists(): return
        base=ROOT/'workloads'/w['name']
        command=['taskset','-c',str(cpus[i%len(cpus)]),str(REPO/'target/debug/rune-eval'),
                 '--contract',str(base/'contract.json'),'--baseline',str(base/'baseline'),'--candidate',str(base/s),
                 '--workdir',str(ROOT/'scratch'/w['name']/s),'--out',str(out),'--allow-degraded']
        p=subprocess.run(command,capture_output=True,text=True,timeout=330)
        if p.returncode not in [0,1] or not out.exists(): raise RuntimeError(p.stderr)
        r=json.loads(out.read_text())
        if r['verdict'].get('reject',{}).get('reason') in ['build_failed','timeout','missing_evidence']:
            raise RuntimeError(str(r['verdict']))
        print(w['name'],s,r['verdict']['status'],round(r['speedup_mean'],3),flush=True)
    # Assign each core one sequential queue: dynamic workers must not collide.
    def queue(k):
        for i in range(k,len(tasks),len(cpus)): worker((i,tasks[i]))
    with cf.ThreadPoolExecutor(max_workers=len(cpus)) as pool:
        for f in [pool.submit(queue,k) for k in range(len(cpus))]:f.result()

def accepted(w,s):
    r=json.loads((ROOT/'reports'/w['name']/(s+'.json')).read_text())
    return r['verdict']['status']=='accept',r

def select():
    verify_freeze()
    assert not (ROOT/'selection.json').exists(), 'selection already frozen'
    assert not any((ROOT/'reports').glob('test_*/*.json')), 'test outcomes already exist'
    ws=json.loads((ROOT/'manifest.json').read_text()); recipes=[]; counts={s:0 for s in STRATEGIES}
    for w in ws:
        if not w['training']:continue
        options=[]
        for s in STRATEGIES:
            ok,r=accepted(w,s); counts[s]+=ok
            if ok:options.append((r['speedup_mean'],s,r))
        if not options:continue
        _,s,r=sorted(options,key=lambda v:(-v[0],v[1]))[0]
        recipes.append(dict(id='',parent=None,proposer='training-evaluator',descriptor=s,fingerprint=w['fingerprint'],
                            template=dict(find_pattern='',replace_pattern='',description='study algorithm'),
                            baseline_hash=r['baseline_digest'],candidate_hash=r['candidate_digest'],
                            evaluation_version='rune-eval 0.2.0',evidence=[dict(sample_path=f"reports/{w['name']}/{s}.json",
                            ci95_low=r['speedup_ci95_low'],ci95_high=r['speedup_ci95_high'],samples=r['samples'],cost_micro_usd=0)],
                            failures=[],retention='standard',status='proposed',tenant='transfer-study',visibility='private'))
    assert recipes, 'no training recipe accepted'
    tests=[w for w in ws if not w['training']]
    request=dict(recipes=recipes,queries=[w['fingerprint'] for w in tests])
    ranked=json.loads(subprocess.check_output([str(REPO/'target/debug/rune-rank-study')],input=json.dumps(request),text=True))
    selected={w['name']:r[0] for w,r in zip(tests,ranked)}
    result=dict(recipes=recipes,choices=selected,training_accept_counts=counts,
                fixed_control=sorted(STRATEGIES,key=lambda s:(-counts[s],s))[0])
    dump(ROOT/'selection.json',result)
    dump(ROOT/'selection-freeze.json',dict(sha256=digest(ROOT/'selection.json')))
    print('frozen memory',len(recipes),'prototypes',sorted(set(r['descriptor'] for r in recipes)),counts)

def wilson(p,n):
    z=1.959963984540054; den=1+z*z/n; c=(p+z*z/(2*n))/den
    h=z*((p*(1-p)/n+z*z/(4*n*n))**0.5)/den
    return [max(0,c-h),min(1,c+h)]

def pvalue(d):
    rng=random.Random(8675309); obs=sum(d); count=0
    for _ in range(100000):count+=sum(x if rng.getrandbits(1) else -x for x in d)>=obs-1e-12
    return (count+1)/100001

def analyze():
    verify_freeze()
    assert digest(ROOT/'selection.json')==json.loads((ROOT/'selection-freeze.json').read_text())['sha256']
    memory=json.loads((ROOT/'selection.json').read_text()); ws=json.loads((ROOT/'manifest.json').read_text())
    rows=[]
    for w in ws:
        if w['training']:continue
        outcomes={s:accepted(w,s)[0] for s in STRATEGIES}
        rows.append(dict(name=w['name'],choice=memory['choices'][w['name']],outcomes=outcomes,
                         memory=int(outcomes[memory['choices'][w['name']]]),random=sum(outcomes.values())/3,
                         fixed=int(outcomes[memory['fixed_control']])))
    n=len(rows); rates={a:sum(r[a] for r in rows)/n for a in ['memory','random','fixed']}
    cis={a:wilson(p,n) for a,p in rates.items()}
    ps={a:pvalue([r['memory']-r[a] for r in rows]) for a in ['random','fixed']}
    sufficient=n>=20 and len(set(r['descriptor'] for r in memory['recipes']))>=2
    better=sufficient and all(rates['memory']>=rates[a]+0.10 and ps[a]<0.05 and cis['memory'][0]>cis[a][1] for a in ['random','fixed'])
    result=dict(verdict='Better' if better else 'NoDifference' if sufficient else 'Insufficient',n=n,
                rates=rates,wilson95=cis,p_values=ps,fixed_control=memory['fixed_control'],
                scope='prospective within-family synthetic sorting selection; not general self improvement',rows=rows)
    dump(ROOT/'result.json',result);print(json.dumps({k:v for k,v in result.items() if k!='rows'},indent=2))

if __name__=='__main__':
    {'generate':generate,'train':lambda:evaluate('train'),'select':select,
     'test':lambda:evaluate('test'),'analyze':analyze}[sys.argv[1]]()
