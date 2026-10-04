#!/usr/bin/env python3
"""Native owner/admin validation on mini1; only dedicated fixture identities.

The operator helper provisions dedicated accounts before this test. The native
scenario cleans up fixture credentials, accounts, registrations and room
memberships at exit. A private journal supports recovery after an interruption.
The workflow SQLite database is isolated from production.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shlex
import subprocess
import tarfile
import time
from urllib.parse import quote
import uuid
from live_palpo import SSH, request
from native_palpo import launch, fill, port


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--palpo', type=Path, required=True)
    parser.add_argument('--root', type=Path, help='Resume provisioning from an existing private journal')
    parser.add_argument('--binary', type=Path, required=True)
    args = parser.parse_args()
    root = (args.root or Path('target/palpo-live') / ('admin-' + uuid.uuid4().hex)).resolve()
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    if not (root / 'operator.json').exists():
        remote = '/Users/cloud/rinx-miniapp-validation/admin-' + uuid.uuid4().hex
        (root / 'operator.json').write_text(json.dumps({'remote': remote, 'state': remote + '/accounts.json'}))
    operator = json.loads((root / 'operator.json').read_text())
    remote = operator['remote']
    home = 'https://crew.ominix.io:19443'

    def ssh(command, **kwargs):
        return subprocess.run([*SSH, 'mini1', command], capture_output=True, timeout=45, **kwargs)

    def remote_json(path):
        result = ssh('cat ' + shlex.quote(path))
        assert result.returncode == 0, 'Private validation file unavailable'
        return json.loads(result.stdout)

    provision = ssh('sudo -n /opt/homebrew/bin/python3 - create ' + shlex.quote(operator['state']),
                    input=Path('tools/wechat-ux/live/mini1_accounts.py').read_bytes())
    if provision.returncode:
        (root / 'operator.log').write_bytes(provision.stdout + provision.stderr)
        raise RuntimeError('Provisioning failed; private recovery journal: ' + str(root))
    accounts = remote_json(operator['state'])['accounts']
    sessions = {}
    for role in ('owner', 'admin', 'bot'):
        assert accounts[role]['user_id'].startswith('@rinx_validation_' + role + '_')
        target = root / (role + '-session.json')
        fd = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, 'w') as f:
            json.dump({'client_session': {'homeserver': home}, 'user_session': accounts[role]}, f)
        sessions[role] = target
    report = {'passed': False, 'matrix': home, 'isolated_backend': remote, 'checks': [],
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'script_sha256': hashlib.sha256(Path('apps/palpo/bundle/main.splash').read_bytes()).hexdigest()}
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w:gz') as tar:
        for path in [args.palpo / 'web-admin/server.mjs', *sorted((args.palpo / 'web-admin/lib').glob('*.mjs')),
                     args.palpo / 'web-admin/test/miniapp-live-server.mjs']:
            tar.add(path, arcname=str(path.relative_to(args.palpo / 'web-admin')), recursive=False)
    result = ssh('umask 077; tar -xz -C ' + shlex.quote(remote), input=archive.getvalue())
    assert result.returncode == 0, 'Upload failed'
    service_port = port()
    endpoint = f'http://127.0.0.1:{service_port}'
    log = (root / 'sidecar.log').open('w')
    server = None
    pid = None
    apps = []
    backend = None
    try:
        command = 'exec /opt/homebrew/bin/node ' + ' '.join(map(shlex.quote, [remote + '/test/miniapp-live-server.mjs',
            remote + '/state', str(service_port), 'http://127.0.0.1:18010', 'crew.ominix.io', operator['state']]))
        server = subprocess.Popen([*SSH, '-o', 'ExitOnForwardFailure=yes', '-L',
                                   f'127.0.0.1:{service_port}:127.0.0.1:{service_port}', 'mini1', command],
                                  stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
        for _ in range(150):
            if server.poll() is not None:
                raise RuntimeError('Sidecar exited; inspect sidecar.log')
            ready = [line for line in (root / 'sidecar.log').read_text().splitlines() if line.startswith('{"ready":true')]
            if ready:
                pid = json.loads(ready[-1])['pid']; break
            time.sleep(.1)
        assert pid, 'Sidecar startup timeout'
        manifest = json.loads(Path('apps/palpo/bundle/manifest.json').read_text())
        tokens = {}
        for role in ('owner', 'admin'):
            code, identity = request(home + '/_matrix/client/v3/account/whoami', accounts[role]['access_token'])
            assert code == 200 and identity['user_id'] == accounts[role]['user_id']
            code, result = request(endpoint + '/_palpo/miniapp/v1/session', accounts[role]['access_token'], {
                'appId': manifest['id'], 'bundleDigest': manifest['integrity']['bundle_blake3'],
                'services': [s for s in manifest['capabilities'] if s.startswith('palpo.')]})
            assert code == 200 and result['isAdmin'] == (role == 'admin'), ('Live role', role, code)
            tokens[role] = result['sessionToken']

        def call(role, service, arguments, expected=200):
            code, result = request(endpoint + '/_palpo/miniapp/v1/call', tokens[role],
                                   {'service': 'palpo.' + service, 'args': arguments})
            assert code == expected, (role, service, code, result.get('code'))
            return result

        def wait_backend(predicate, label, timeout=45):
            nonlocal backend
            end = time.time() + timeout
            while time.time() < end:
                backend = remote_json(remote + '/state/backend.json')
                if predicate(backend):
                    return backend
                time.sleep(.5)
            raise AssertionError(label + ': timed out; inspect backend.json')

        call('owner', 'activity.list', {}, 403)
        report['checks'].append('real Matrix admin and member sessions preserve server roles; owner cannot use admin service')
        print('Live roles verified', flush=True)
        owner = launch(root / 'owner', args.binary, endpoint, narrow=True, session_file=sessions['owner']); apps.append(owner)
        owner.click_id('contribute'); owner.wait_text('Resource pool name')
        name = 'Live resource ' + uuid.uuid4().hex[:8]
        fill(owner, 'Resource pool name', name)
        fill(owner, 'What can you contribute?', 'Dedicated Rinx validation resource; no production compute.')
        owner.click_id('submit'); owner.wait_text('Open latest result')
        action = call('owner', 'inbox.list', {'view': 'waiting'})['actions'][0]
        action_id = action['id']
        call('owner', 'inbox.decide', {'id': action_id, 'expectedRevision': 1, 'commandId': uuid.uuid4().hex,
                                     'decision': 'approve', 'reason': 'Cannot self approve'}, 403)

        # Read/seen state must not remove a pending approval or suppress reminders.
        wait_backend(lambda b: any(n['actionId'] == action_id and n['recipient'] == accounts['admin']['user_id']
                                   and n['delivered'] >= 1 for n in b['notices']), 'first admin notice')
        call('admin', 'inbox.seen', {'id': action_id})
        assert call('admin', 'inbox.get', {'id': action_id})['action']['needsMyAction']
        wait_backend(lambda b: any(n['actionId'] == action_id and n['recipient'] == accounts['admin']['user_id']
                                   and n['seenAt'] and n['delivered'] >= 2 for n in b['notices']), 'reminder after seen')
        for role in ('owner', 'admin'):
            room = next(r for r in backend['rooms'] if r['ownerMxid'] == accounts[role]['user_id'])['roomId']
            code, _ = request(home + '/_matrix/client/v3/join/' + quote(room, safe=''), accounts[role]['access_token'], {})
            assert code == 200, ('Join own private action room', role, code)
            code, messages = request(home + '/_matrix/client/v3/rooms/' + quote(room, safe='') + '/messages?dir=b&limit=30', accounts[role]['access_token'])
            assert code == 200
            notices = [e for e in messages['chunk'] if e.get('type') == 'm.room.message' and e.get('content', {}).get('im.palpo.action.v1')]
            assert notices and all(e['content']['im.palpo.action.v1']['id'] == action_id for e in notices)
            raw = json.dumps(notices)
            assert name not in raw and 'as_token' not in raw and 'hs_token' not in raw
            assert all(a['access_token'] not in raw for a in accounts.values())
            assert home + '/_palpo/miniapp/action/' + action_id in raw
        report['checks'].append('real private Matrix action rooms receive minimal notices and a reminder after seen; action remains pending')
        print('Real Matrix notices and seen/reminder behavior verified', flush=True)

        admin = launch(root / 'admin', args.binary, endpoint, admin=True, session_file=sessions['admin']); apps.append(admin)
        admin.wait_text(name); admin.click_id('review'); admin.wait_text('Approve')
        admin.capture('live-admin-review')
        admin.click_id('approve'); admin.wait_text('Decision reason')
        fill(admin, 'Decision reason', 'Approved dedicated validation resource')
        admin.click_id('submit'); admin.wait_text('No requests in this view', timeout=30)
        current = call('owner', 'inbox.get', {'id': action_id})['action']
        assert current['state'] == 'approved' and current['execution'] == 'done'
        fleet = current['result']['fleetId']
        owner.click_id('needs'); owner.wait_text(name); owner.click_id('review'); owner.wait_text('Save configuration')
        owner.capture('live-owner-config-handoff')
        # Owner-only download response never enters the mini-app script. The
        # harness checks authorization over HTTP; native save dialog is separate.
        config = call('owner', 'fleets.export', {'fleetId': fleet})
        assert config
        # The backend hides another owner's configuration with a 404.
        call('admin', 'fleets.export', {'fleetId': fleet}, 404)
        call('admin', 'inbox.decide', {'id': action_id, 'expectedRevision': 1, 'commandId': uuid.uuid4().hex,
                                     'decision': 'approve', 'reason': 'Stale duplicate'}, 409)
        wait_backend(lambda b: any(n['actionId'] == action_id and n['recipient'] == accounts['owner']['user_id']
                                   and n['revision'] == current['revision'] and n['delivered'] >= 1 for n in b['notices']), 'owner handoff notice')
        report['checks'].append('native owner request and native administrator approval install a real Matrix App Service; export is owner-only; stale approval is rejected')
        print('Native live approval and owner-only handoff verified', flush=True)

        # A second native request follows the rejection branch; no registration.
        owner.click_id('inbox'); owner.click_id('contribute'); owner.wait_text('Resource pool name')
        rejected_name = 'Rejected validation ' + uuid.uuid4().hex[:8]
        fill(owner, 'Resource pool name', rejected_name)
        fill(owner, 'What can you contribute?', 'Rejection branch validation')
        owner.click_id('submit'); owner.wait_text('Open latest result')
        rejected = call('owner', 'inbox.list', {'view': 'waiting'})['actions'][0]
        admin.click_id('inbox'); admin.wait_text(rejected_name); admin.click_id('review'); admin.click_id('reject')
        admin.wait_text('Decision reason'); fill(admin, 'Decision reason', 'Rejected validation request')
        admin.click_id('submit'); admin.wait_text('No requests in this view')
        assert call('owner', 'inbox.get', {'id': rejected['id']})['action']['state'] == 'rejected'
        owner.click_id('inbox'); owner.click_id('history'); owner.wait_text(rejected_name)
        owner.capture('live-owner-rejection-history')
        report['checks'].append('native administrator rejection returns to owner history without installing another App Service')
        owner.click_id('disconnect'); owner.wait_text('Rinx remains signed in')
        code, _ = request(home + '/_matrix/client/v3/account/whoami', accounts['owner']['access_token'])
        assert code == 200
        report['checks'].append('mini-app disconnect preserves the underlying Matrix login')
        for app in apps:
            errors = [line for line in (app.output / 'native.log').read_text().splitlines()
                      if '[E]' in line or 'on_render closure failed' in line or 'callback error' in line]
            assert not errors, 'Native script errors; inspect private native.log'
        report['passed'] = True
    finally:
        for app in apps:
            try:
                app.capture('final-state')
            except Exception:
                pass
            app.stop()
        if pid:
            ssh('kill -TERM ' + str(pid))
        if server:
            try:
                server.wait(timeout=15)
            except subprocess.TimeoutExpired:
                server.terminate(); server.wait(timeout=10)
        log.close()
        if pid:
            try:
                backend = remote_json(remote + '/state/backend.json')
            except Exception:
                pass
        if backend:
            (root / 'backend.json').write_text(json.dumps(backend, indent=2))
        cleanup = ssh('sudo -n /opt/homebrew/bin/python3 - cleanup ' + shlex.quote(operator['state']),
                      input=Path('tools/wechat-ux/live/mini1_accounts.py').read_bytes())
        report['cleanup_passed'] = cleanup.returncode == 0
        (root / 'cleanup.log').write_bytes(cleanup.stdout + cleanup.stderr)
        for path in sessions.values():
            path.unlink(missing_ok=True)
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(dict(report, evidence=str(root)), indent=2))
        assert report['cleanup_passed'], 'Fixture cleanup failed; see private cleanup.log'


if __name__ == '__main__':
    main()
