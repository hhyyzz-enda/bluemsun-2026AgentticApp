#!/usr/bin/env python3
"""Actual production Splash + Rinx host adapter + local Palpo backend.

No live account or deployment is used. Matrix is the Palpo test fixture;
SQLite workflows, HTTP sessions, Rinx transport, widgets and inputs are real.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import uuid
import urllib.error
from PIL import Image
from native_probe import NativeApp

class PalpoApp(NativeApp):
    def snap(self):
        return [w for w in super().snap() if w.get('ty') != 'Splash']

    def request(self, route, **params):
        # SDK 1f3b1de can fail wait=1 after already applying the input when a
        # hidden Metal frame cannot immediately submit. Never replay that input.
        # A separate read-only grab is the frame barrier instead.
        barrier = route in {'/click', '/m', '/k', '/t'} and params.get('wait') == 1
        if barrier:
            params['wait'] = 0
        try:
            result = super().request(route, **params)
            if barrier:
                time.sleep(.05)
                super().request('/g')
            return result
        except urllib.error.HTTPError as error:
            detail = error.read().decode('utf-8', errors='replace')
            self.trace.append({'bridge_error': route, 'status': error.code, 'detail': detail})
            raise NativeBridgeError(f'{route}: HTTP {error.code}: {detail}') from error


class NativeBridgeError(RuntimeError):
    pass



def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]


def launch(root, binary, endpoint, admin=False, narrow=False, session_file=None):
    profile = root / 'profile'
    (profile / 'app').mkdir(parents=True, exist_ok=True)
    app = PalpoApp(root, port=port(), auto_login=False)
    app.output.mkdir(parents=True)
    app.log = (app.output / 'native.log').open('w')
    env = dict(os.environ, RINX_DATA_DIR=str(profile.resolve()), MAKEPAD_HIDE_WINDOWS='1',
               MAKEPAD_NO_FOCUS='1', MAKEPAD_REMOTE=str(app.port), PALPO_FIXTURE_URL=endpoint)
    env.pop('MAKEPAD_FOCUS', None)
    env.pop('PALPO_LIVE_SESSION_FILE', None)
    if session_file:
        env['PALPO_LIVE_SESSION_FILE'] = str(session_file.resolve())
    args = [str(binary.resolve())] + (['--admin'] if admin else []) + (['--narrow'] if narrow else [])
    app.process = subprocess.Popen(args, env=env, stdout=app.log, stderr=subprocess.STDOUT)
    for _ in range(120):
        if app.process.poll() is not None:
            raise RuntimeError(f'Native fixture exited; see {app.output}')
        try:
            status = app.request('/s')
            if status['w']:
                app.wait_text('Contribute resources', timeout=10)
                return app
        except (OSError, NativeBridgeError):
            pass
        time.sleep(.1)
    app.stop()
    raise RuntimeError(f'Native bridge did not draw: {app.output}')


def fill(app, label, value):
    tree = app.request('/snap', all=1)['s']
    fields = [w for w in tree if w.get('ty') == 'TextInput' and w['r'][2] > 0]
    (app.root / 'last-fields.json').write_text(json.dumps(tree, indent=2))
    # Fields have their labels immediately above them; visual order is stable.
    labels = [w for w in tree if w.get('t') == label and w.get('ty') != 'TextInput']
    assert labels, (label, app.snap())
    y = labels[-1]['r'][1]
    target = min((w for w in fields if w['r'][1] >= y), key=lambda w: w['r'][1])
    x, y, width, height = target['r']
    app.click(x + width / 2, y + height / 2)
    app.request('/k', c='KeyA', cmd=1, wait=1)
    app.request('/t', t=value, wait=1)


def inspect(app):
    app.request('/event', data='palpo:inspect', wait=1)
    return json.loads((app.root / 'profile/inspection.json').read_text())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--palpo', type=Path, required=True)
    parser.add_argument('--node', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/fast/examples/palpo_miniapp'))
    parser.add_argument('--smoke', action='store_true')
    args = parser.parse_args()
    root = Path('target/palpo-validation') / uuid.uuid4().hex
    root.mkdir(parents=True)
    service_port = port()
    log = (root / 'server.log').open('w')
    server = subprocess.Popen([str(args.node), str(args.palpo / 'web-admin/test/miniapp-native-fixture.mjs'), str(service_port), str(root.resolve())], stdout=log, stderr=subprocess.STDOUT)
    apps = []
    report = {'passed': False, 'evidence': str(root.resolve()), 'checks': [], 'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest()}
    try:
        for _ in range(80):
            if 'ready' in (root / 'server.log').read_text(): break
            if server.poll() is not None: raise RuntimeError((root / 'server.log').read_text())
            time.sleep(.1)
        endpoint = f'http://127.0.0.1:{service_port}'
        owner = launch(root / 'owner', args.binary, endpoint, narrow=True); apps.append(owner)
        owner.capture('owner-inbox-light')
        owner.click_id('contribute'); owner.wait_text('Resource pool name')
        fill(owner, 'Resource pool name', 'Research pool')
        fill(owner, 'What can you contribute?', 'Shared coding capacity')
        owner.capture('contribution-draft')
        owner.request('/k', c='ArrowLeft', shift=1, wait=1)
        owner.request('/k', c='ArrowLeft', shift=1, wait=1)
        before = inspect(owner)
        for theme in ('dark', 'violet', 'light'):
            owner.request('/event', data='palpo:' + theme, wait=1)
            time.sleep(.25)
            owner.wait_text('Research pool')
            after = inspect(owner)
            assert (before['heap'], before['calls']) == (after['heap'], after['calls']), (before, after)
            capture = owner.capture('contribution-' + theme)
            im = Image.open(capture).convert('RGB')
            bright = sum(1 for rgb in im.getdata() if sum(rgb) > 540) / (im.width * im.height)
            assert (bright < .2 if theme == 'dark' else bright > .65), (theme, bright)

        owner.request('/t', t='XY', wait=1)
        edited = json.loads((owner.root / 'profile/app/draft.json').read_text())
        assert edited['payload']['reason'] == 'Shared coding capaciXY', edited
        owner.request('/k', c='KeyZ', cmd=1, wait=1)
        restored = json.loads((owner.root / 'profile/app/draft.json').read_text())
        assert restored['payload']['reason'] == 'Shared coding capacity', restored
        report['checks'].append('live theme changes preserve draft, focus, selection, undo, isolate and request count; dark pixels verified')
        if not args.smoke:
            # A process restart restores the same draft and trusted request ID.
            draft_before = json.loads((owner.root / 'profile/app/draft.json').read_text())
            owner.stop(); apps.remove(owner)
            owner = launch(root / 'owner', args.binary, endpoint, narrow=True); apps.append(owner)
            owner.click_id('resume'); owner.wait_text('Research pool')
            assert json.loads((owner.root / 'profile/app/draft.json').read_text()) == draft_before
            report['checks'].append('draft and idempotency key survive process restart')
            owner.click_id('submit'); owner.wait_text('Open latest result')
            admin = launch(root / 'admin', args.binary, endpoint, admin=True); apps.append(admin)
            admin.wait_text('Research pool'); admin.click_id('review'); admin.wait_text('Approve')
            admin.click_id('approve'); admin.wait_text('Decision reason')
            fill(admin, 'Decision reason', 'Approved for research')
            admin.click_id('submit'); admin.wait_text('No requests in this view')
            owner.click_id('needs'); owner.wait_text('Research pool'); owner.click_id('review'); owner.wait_text('Save configuration')
            owner.capture('contribution-approved-owner-handoff')
            report['checks'].append('owner contribution and administrator approval through real HTTP/SQLite')
            owner.click_id('resources'); owner.wait_text('Request project here'); owner.click_id('choose')
            owner.wait_text('Project name'); fill(owner, 'Project name', 'Native test project')
            fill(owner, 'What will your project do?', 'Run a research agent')
            owner.click_id('submit'); owner.wait_text('Native test project')
            admin.click_id('inbox'); admin.wait_text('Native test project'); admin.click_id('review')
            admin.click_id('approve'); admin.wait_text('Decision reason'); fill(admin, 'Decision reason', 'Project approved')
            admin.click_id('submit'); admin.wait_text('No requests in this view')
            owner.click_id('needs'); owner.wait_text('Native test project')
            # Two cards exist (contribution handoff and project activation).
            reviews = [w for w in owner.snap() if w['i'] == 'review']
            target = min(reviews, key=lambda w: w['r'][1])
            x, y, w, h = target['r']; owner.click(x+w/2, y+h/2)
            owner.wait_text('Continue approved work'); owner.click_id('continue_work')
            owner.wait_text('Latest result received.')
            owner.click_id('projects'); owner.wait_text('Native test project'); owner.click_id('agent')
            owner.wait_text('Use this resource'); owner.click_id('choose'); owner.wait_text('Agent name')
            fill(owner, 'Agent name', 'ResearchBot')
            # Scroll the form's last fields and submit into the viewport.
            owner.request('/m', k='scroll', x=300, y=650, dy=350, wait=1)
            owner.click_id('submit'); owner.wait_text('Initial request:')
            owner.capture('agent-request-pending')
            backend = json.loads((root / 'backend.json').read_text())
            assert backend['projects'] == 1 and backend['requests'] == 1, backend
            report['checks'].append('project approval, owner activation and named agent request through native forms')
            owner.click_id('disconnect'); owner.wait_text('Rinx remains signed in')
            assert json.loads((root / 'backend.json').read_text())['logouts'] == 0
            report['checks'].append('mini-app disconnect preserves Matrix login')

        for app in apps:
            errors = [line for line in (app.output / 'native.log').read_text().splitlines()
                      if '[E]' in line or 'on_render closure failed' in line or 'callback error' in line]
            assert not errors, errors
        report['passed'] = True
    finally:
        for app in apps:
            try:
                app.capture('final-state')
                (app.root / 'final-tree.json').write_text(json.dumps(app.request('/snap', all=1), indent=2))
            except Exception: pass
            finally: app.stop()
        server.terminate()
        try: server.wait(timeout=8)
        except subprocess.TimeoutExpired: server.kill(); server.wait()
        log.close()
        (root / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))

if __name__ == '__main__':
    main()
