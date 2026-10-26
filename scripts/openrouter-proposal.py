"""One bounded synthetic discovery request. Never applies returned code."""
import json
import os
import sys
import urllib.request

MODEL = 'moonshotai/kimi-k3.1-pro'

def main():
    key = os.environ.get('OPENROUTER_API_KEY')
    if not key:
        print(json.dumps({'status': 'blocked', 'reason': 'credential_not_injected'}))
        return 2
    # Synthetic content only; no workspace files or hidden evaluator material.
    payload = {
        'model': MODEL, 'max_tokens': 1000, 'temperature': 0,
        'provider': {'data_collection': 'deny', 'zdr': True,
                     'require_parameters': True, 'allow_fallbacks': False},
        'messages': [
            {'role': 'system', 'content': 'Return a JSON object with exactly keys proposal and risks. Both values must be strings. Suggest one Rust build optimization. Do not output shell commands.'},
            {'role': 'user', 'content': 'Synthetic fixture: a Rust library computes squared Euclidean distances over f32 arrays. Propose a measurable optimization and its semantic risks. This is discovery only.'}
        ]
    }
    request = urllib.request.Request(
        'https://openrouter.ai/api/v1/chat/completions',
        data=json.dumps(payload).encode(),
        headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            data = json.load(response)
        proposal = json.loads(data['choices'][0]['message']['content'])
        if not isinstance(proposal, dict) or set(proposal) != {'proposal', 'risks'}:
            raise ValueError('schema')
        if not all(isinstance(v, str) and len(v) <= 20000 for v in proposal.values()):
            raise ValueError('schema')
        print(json.dumps({'status': 'proposal_only', 'model': data.get('model'),
                          'id': data.get('id'), 'usage': data.get('usage'),
                          'candidate': proposal, 'validated': False}))
        return 0
    except Exception:
        # Never print request objects, headers, secret values or provider bodies.
        print(json.dumps({'status': 'rejected', 'reason': 'provider_or_schema_failure'}))
        return 1

if __name__ == '__main__':
    sys.exit(main())
