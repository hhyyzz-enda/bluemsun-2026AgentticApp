#!/usr/bin/env python3
"""Explicit operator test: real crew Matrix session, isolated backend on mini1.

Credentials are read from the specified saved Rinx session and held in memory.
They never enter arguments, reports, bundle storage, or the production admin DB.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shlex
import subprocess
import tarfile
import time
import urllib.error
import urllib.request
import uuid
from native_palpo import launch, fill, inspect, port

SSH = ['ssh', '-o', 'BatchMode=yes', '-o', 'UseKeychain=yes', '-o', 'ConnectTimeout=10']


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def request(url, token=None, body=None):
    headers = {'Content-Type': 'application/json'}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    req = urllib.request.Request(url, headers=headers,
                                 data=None if body is None else json.dumps(body).encode())
    try:
        with urllib.request.build_opener(NoRedirect).open(req, timeout=20) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        try:
            data = json.load(error)
        except ValueError:
            data = {}
        return error.code, data


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--palpo', type=Path, required=True)
    parser.add_argument('--session-file', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    session = json.loads(args.session_file.read_text())
    home = session['client_session']['homeserver'].rstrip('/')
    assert home == 'https://crew.ominix.io:19443'
    user = session['user_session']
    token = user.get('tokens', user)['access_token']
    actor = user.get('meta', user)['user_id']
    run = uuid.uuid4().hex
    root = Path('target/palpo-live') / run
    root.mkdir(parents=True, mode=0o700)
    remote = '/Users/cloud/rinx-miniapp-validation/' + run
    service_port = port()
    endpoint = f'http://127.0.0.1:{service_port}'
    report = {'passed': False, 'matrix': home, 'admin': 'https://crew.ominix.io:19444',
              'account': actor, 'isolated_backend': remote, 'checks': [],
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'script_sha256': hashlib.sha256(Path('apps/palpo/bundle/main.splash').read_bytes()).hexdigest()}
    # Only application code enters the sidecar. Never copy credentials or a DB.
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w:gz') as tar:
        for path in [args.palpo / 'web-admin/server.mjs',
                     *sorted((args.palpo / 'web-admin/lib').glob('*.mjs')),
                     args.palpo / 'web-admin/test/miniapp-live-server.mjs']:
            tar.add(path, arcname=str(path.relative_to(args.palpo / 'web-admin')), recursive=False)
    command = f'umask 077; mkdir -p {shlex.quote(remote)}; tar -xz -C {shlex.quote(remote)}'
    subprocess.run([*SSH, 'mini1', command], input=archive.getvalue(), check=True,
                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    server = None
    apps = []
    remote_pid = None
    log = (root / 'sidecar.log').open('w')
    try:
        status, identity = request(home + '/_matrix/client/v3/account/whoami', token)
        assert status == 200 and identity['user_id'] == actor, status
        # Existing admin web performs its real whoami check before rejecting the
        # deliberately nonexistent fleet. No export or record is created there.
        missing_fleet = report['admin'] + '/api/pair/hf_' + '0' * 32
        invalid_status, _ = request(missing_fleet, 'rinx_validation_invalid_' + run, {})
        assert invalid_status == 401, invalid_status
        status, _ = request(missing_fleet, token, {})
        assert status == 404, status
        report['checks'].append('current Matrix login and existing admin web upstream authenticate the same member')
        command = 'exec /opt/homebrew/bin/node ' + shlex.quote(remote + '/test/miniapp-live-server.mjs')
        command += ' ' + ' '.join(map(shlex.quote, [remote + '/state', str(service_port), 'http://127.0.0.1:18010', 'crew.ominix.io']))
        server = subprocess.Popen([*SSH, '-o', 'ExitOnForwardFailure=yes', '-L',
                                   f'127.0.0.1:{service_port}:127.0.0.1:{service_port}', 'mini1', command],
                                  stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
        for _ in range(150):
            if server.poll() is not None:
                raise RuntimeError('Sidecar exited; inspect sidecar.log')
            ready = [line for line in (root / 'sidecar.log').read_text().splitlines() if line.startswith('{"ready":true')]
            if ready:
                remote_pid = json.loads(ready[-1])['pid']; break
            time.sleep(.1)
        assert remote_pid, 'Sidecar startup timeout'
        manifest = json.loads(Path('apps/palpo/bundle/manifest.json').read_text())
        def login():
            code, result = request(endpoint + '/_palpo/miniapp/v1/session', token,
                                   {'appId': manifest['id'], 'bundleDigest': manifest['integrity']['bundle_blake3'],
                                    'services': [s for s in manifest['capabilities'] if s.startswith('palpo.')]})
            assert code == 200 and result['userId'] == actor and not result['isAdmin'], code
            return result['sessionToken']
        app_token = login()
        code, _ = request(endpoint + '/_palpo/miniapp/v1/call', app_token, {'service':'palpo.activity.list', 'args':{}})
        assert code == 403, code
        report['checks'].append('live Matrix session exchange succeeds; member admin operation is refused')
        owner = launch(root / 'owner', args.binary, endpoint, narrow=True, session_file=args.session_file)
        apps.append(owner)
        owner.wait_text(actor)
        owner.capture('live-member-inbox')
        owner.click_id('contribute'); owner.wait_text('Resource pool name')
        fill(owner, 'Resource pool name', 'Rinx live validation ' + run[:8])
        fill(owner, 'What can you contribute?', 'Isolated workflow validation; no production allocation requested.')
        before = inspect(owner)
        owner.request('/event', data='palpo:dark', wait=1); time.sleep(.3)
        owner.capture('live-member-dark-draft')
        after = inspect(owner)
        assert (before['heap'], before['calls']) == (after['heap'], after['calls'])
        owner.click_id('submit'); owner.wait_text('Open latest result')
        owner.click_id('review'); owner.wait_text('requested')
        assert not any(w['i'] in ('approve', 'reject') for w in owner.snap())
        owner.capture('live-member-request')
        code, inbox = request(endpoint + '/_palpo/miniapp/v1/call', app_token,
                              {'service':'palpo.inbox.list', 'args':{'view':'waiting'}})
        assert code == 200 and inbox['total'] == 1
        assert inbox['actions'][0]['ownerMxid'] == actor
        report['checks'].append('actual native UI uses live identity, saves a contribution in isolated SQLite, and hides admin decisions')
        owner.click_id('disconnect'); owner.wait_text('Rinx remains signed in')
        status, identity = request(home + '/_matrix/client/v3/account/whoami', token)
        assert status == 200 and identity['user_id'] == actor
        report['checks'].append('native mini-app disconnect preserves the real Matrix session')
        errors = [line for line in (owner.output / 'native.log').read_text().splitlines()
                  if '[E]' in line or 'on_render closure failed' in line or 'callback error' in line]
        assert not errors, 'Native script errors; inspect the private native.log'
        report['passed'] = True
    finally:
        for app in apps:
            app.stop()
        if remote_pid:
            subprocess.run([*SSH, 'mini1', f'kill -TERM {int(remote_pid)}'], capture_output=True, timeout=20)
        if server:
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.terminate(); server.wait(timeout=10)
        log.close()
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(dict(report, evidence=str(root.resolve())), indent=2))


if __name__ == '__main__':
    main()
