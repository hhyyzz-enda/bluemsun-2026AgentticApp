#!/usr/bin/env python3
"""Explicit operator helper, run on mini1: provision isolated validation users.

Uses Palpo's supported no-server CLI and HTTP admin API. Never reads an existing
user's credentials, joins production rooms, or prints generated credentials.
The private state file is also the cleanup journal for interrupted runs.
"""
import argparse
import json
import os
from pathlib import Path
import plistlib
import pwd
import re
import secrets
import subprocess
import urllib.error
import urllib.parse
import urllib.request

HOME_SERVER = 'https://crew.ominix.io:19443'
UPSTREAM = 'http://127.0.0.1:18010'


def request(path, token=None, body=None, method=None):
    headers = {'Content-Type': 'application/json'}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    req = urllib.request.Request(UPSTREAM + path, headers=headers, method=method,
                                 data=None if body is None else json.dumps(body).encode())
    try:
        with urllib.request.urlopen(req, timeout=30) as result:
            return result.status, json.load(result)
    except urllib.error.HTTPError as error:
        return error.code, {'errcode': json.load(error).get('errcode')}


def save(path, state):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.write_text(json.dumps(state))
    path.chmod(0o600)
    cloud = pwd.getpwnam('cloud')
    os.chown(path, cloud.pw_uid, cloud.pw_gid)
    os.chown(path.parent, cloud.pw_uid, cloud.pw_gid)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['create', 'cleanup'])
    parser.add_argument('state', type=Path)
    args = parser.parse_args()
    path = args.state.resolve()
    assert path.is_relative_to('/Users/cloud/rinx-miniapp-validation')
    if args.action == 'create':
        state = json.loads(path.read_text()) if path.exists() else {
            'run': secrets.token_hex(8), 'accounts': {}, 'fleets': [], 'rooms': []}
        assert re.fullmatch(r'[a-f0-9]{16}', state['run']) and not state.get('cleaned')
        save(path, state)
        env = dict(os.environ)
        with open('/Users/cloud/Library/LaunchAgents/io.ominix.palpo-native.plist', 'rb') as f:
            env.update(plistlib.load(f)['EnvironmentVariables'])
        env.update(PALPO_AUTO_JOIN_ROOMS='[]', PALPO_ADMIN__ROOM_NOTICES='false',
                   PALPO_ADMIN__STARTUP_EXECUTE='[]', PALPO_ADMIN__CONSOLE_AUTOMATIC='false')

        def operator(command):
            result = subprocess.run(['/Users/cloud/palpo-crew/palpo', '--server', 'false',
                                     '--execute', command], env=env, cwd='/Users/cloud/palpo-crew',
                                    capture_output=True, text=True, timeout=60)
            if result.returncode:
                # Operator output can contain generated passwords; never echo it.
                raise RuntimeError('Palpo operator command failed: ' + command.split()[1])
            return result.stdout + result.stderr

        for role in ('admin', 'owner', 'bot'):
            name = 'rinx_validation_' + role + '_' + state['run']
            actor = '@' + name + ':crew.ominix.io'
            prior = state['accounts'].get(role)
            if prior and prior.get('access_token'):
                assert prior['user_id'] == actor
                continue
            state['accounts'][role] = {'user_id': actor}
            save(path, state)
            output = operator(('user reset-password ' if prior else 'user create-user ') + name)
            output = re.sub(r'\x1b\[[0-9;]*[A-Za-z]', '', output)
            # The operator renders Markdown even on a pipe, wrapping long MXIDs.
            output = re.sub(r'\s+', '', output)
            marker = re.escape(actor) + ':' if prior else 'password:'
            password = re.search(marker + r'\s*`?([A-Za-z0-9]{20,25})', output)
            if not password:
                raise RuntimeError('No generated password returned for ' + role)
            if role == 'admin':
                operator('user make-user-admin ' + actor)
            code, session = request('/_matrix/client/v3/login', body={
                'type': 'm.login.password', 'identifier': {'type': 'm.id.user', 'user': actor},
                'password': password[1], 'initial_device_display_name': 'Rinx isolated validation'})
            assert code == 200, ('Matrix login', role, code)
            state['accounts'][role] = {k: session[k] for k in ('user_id', 'device_id', 'access_token')}
            save(path, state)
            admin_token = state['accounts']['admin']['access_token']
            if role != 'admin':
                # First-user CLI bootstrapping can auto-grant admin. Explicitly
                # normalize ONLY this run's identities through the supported API.
                code, _ = request('/_palpo/admin/v2/users/' + urllib.parse.quote(actor, safe=''),
                                  admin_token, {'admin': False}, 'PUT')
                assert code == 200, ('Set member role', role, code)
            code, _ = request('/_palpo/admin/v1/appservices', session['access_token'])
            assert code == (200 if role == 'admin' else 403), ('Role', role, code)
        print(json.dumps({'created': True, 'state': str(path),
                          'roles': list(state['accounts']), 'server_restarted': False}))
    else:
        state = json.loads(path.read_text())
        admin_token = state['accounts']['admin']['access_token']
        # Only fixture-owned objects recorded in this private journal are touched.
        for room in state.get('rooms', []):
            for account in state['accounts'].values():
                if not account.get('access_token'):
                    continue
                request('/_matrix/client/v3/rooms/' + urllib.parse.quote(room, safe='') + '/leave',
                        account['access_token'], {})
                request('/_matrix/client/v3/rooms/' + urllib.parse.quote(room, safe='') + '/forget',
                        account['access_token'], {})
        for fleet in state.get('fleets', []):
            assert re.fullmatch(r'hf_[a-f0-9]{32}', fleet)
            code, _ = request('/_palpo/admin/v1/appservices/' + fleet, admin_token, method='DELETE')
            assert code in (200, 404), ('Unregister fixture', code)
        extra = [{'user_id': '@' + f + '_representative:crew.ominix.io'} for f in state.get('fleets', [])]
        accounts = extra + [a for r, a in state['accounts'].items() if r != 'admin'] + [state['accounts']['admin']]
        for account in accounts:
            actor = account['user_id']
            assert actor.startswith('@rinx_validation_') or actor in [a['user_id'] for a in extra]
            if account.get('cleaned'):
                continue
            code, _ = request('/_palpo/admin/v2/users/' + urllib.parse.quote(actor, safe=''), admin_token)
            if code == 404:
                continue
            assert code == 200, ('Find fixture account', actor, code)
            code, _ = request('/_palpo/admin/v2/users/' + urllib.parse.quote(actor, safe=''), admin_token,
                              {'admin': False, 'deactivated': True, 'locked': True,
                               'password': secrets.token_urlsafe(32), 'logout_devices': True}, 'PUT')
            assert code == 200, ('Deactivate fixture', actor, code)
            account['cleaned'] = True
            save(path, state)
        for account in state['accounts'].values():
            if account.get('access_token'):
                code, _ = request('/_matrix/client/v3/account/whoami', account['access_token'])
                assert code == 401, ('Fixture session invalidated', code)
            account.pop('access_token', None)
        state['cleaned'] = True
        save(path, state)
        path.with_suffix(path.suffix + '.operator').unlink(missing_ok=True)
        print(json.dumps({'cleaned': True, 'roles': list(state['accounts']), 'fleets': len(state.get('fleets', []))}))


if __name__ == '__main__':
    main()
