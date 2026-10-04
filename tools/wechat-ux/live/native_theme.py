#!/usr/bin/env python3
"""ADR 0009: real Makepad hidden-window theme/state regression, offline only.

Build: cargo build --profile fast --locked --example theme_mvp
Run: python3 tools/wechat-ux/live/native_theme.py
Evidence includes native PNGs, widget bounds, state observations and input trace.
"""
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import time
import uuid
from native_probe import NativeApp


def launch(root, hosted=False, narrow=False):
    profile = root / 'profile'
    profile.mkdir(parents=True, exist_ok=True, mode=0o700)
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        port = probe.getsockname()[1]
    app = NativeApp(root, port=port, auto_login=False)
    app.output.mkdir(parents=True, mode=0o700)
    app.log = (app.output / 'native.log').open('w')
    env = dict(os.environ, RINX_DATA_DIR=str(profile.resolve()), MAKEPAD_HIDE_WINDOWS='1',
               MAKEPAD_NO_FOCUS='1', MAKEPAD_REMOTE=str(port))
    env.pop('MAKEPAD_FOCUS', None)
    args = [str(Path('target/fast/examples/theme_mvp').resolve())]
    if hosted:
        args.append('--hosted')
    if narrow:
        args.append('--narrow')
    app.process = subprocess.Popen(args, env=env, stdout=app.log, stderr=subprocess.STDOUT)
    for _ in range(120):
        if app.process.poll() is not None:
            raise RuntimeError(f'Theme fixture exited; inspect {app.output}/native.log')
        try:
            status = app.request('/s')
            assert status['pid'] == app.process.pid
            if status['w']:
                app.wait_text('One theme, three native surfaces')
                return app
        except OSError:
            pass
        time.sleep(.25)
    raise RuntimeError('Native bridge did not start')


def inspect(app):
    app.request('/event', data='theme:inspect', wait=1)
    return json.loads((app.root / 'profile/theme-inspection.json').read_text())


def switch(app, selection):
    app.request('/event', data=f'theme:{selection}', wait=1)
    time.sleep(.45)


def type_at(app, widget, text):
    x, y, width, height = widget['r']
    app.click(x + min(width / 2, 100), y + height / 2)
    app.request('/k', c='KeyA', cmd=1, wait=1)
    app.request('/t', t=text, wait=1)
    app.request('/k', c='ArrowLeft', shift=1, wait=1)
    app.request('/k', c='ArrowLeft', shift=1, wait=1)


def main():
    root = Path('target/theme-mvp-validation') / uuid.uuid4().hex
    root.mkdir(parents=True, mode=0o700)
    report = {'passed': False, 'runs': [], 'binary_sha256': hashlib.sha256(
        Path('target/fast/examples/theme_mvp').read_bytes()).hexdigest()}
    try:
        for hosted in (False, True):
            run = root / ('hosted' if hosted else 'standalone')
            # A standalone preference must not override hosted authority.
            if hosted:
                (run / 'profile').mkdir(parents=True)
                (run / 'profile/appearance.json').write_text('{"appearance":"dark","accent":"violet"}')
            app = launch(run, hosted=hosted)
            try:
                if not hosted:
                    # Exercise the production settings widget, not only the
                    # fixture's host-reload controls.
                    app.click_id('mode')
                    app.request('/k', c='ArrowDown', wait=1)
                    time.sleep(.45)
                    assert inspect(app)['selection']['appearance'] == 'dark'
                    app.click_id('mode')
                    app.request('/k', c='ArrowUp', wait=1)
                    time.sleep(.45)
                    assert inspect(app)['selection']['appearance'] == 'light'
                else:
                    app.wait_text('Appearance is managed by OctoSense')
                    assert not any(w['i'] in ('mode','accent') for w in app.snap())
                original = inspect(app)
                assert original['requests'] == 1 and original['timers'] == 1, original
                assert original['l0_ink'] == original['ink'], original
                assert (original['selection'] is None) == hosted, original
                app.capture('light-teal')
                fields = [w for w in app.snap() if w['i'] == 'draft']
                assert len(fields) == 2, fields
                type_at(app, fields[0], 'Native unsaved 中英 draft')
                type_at(app, fields[1], 'Splash unsaved 中英 draft')
                app.click_id('increment')
                app.wait_text('Count: 1')
                l0 = next(w for w in app.snap() if w['i'] == 'beauty_0_2_0')
                type_at(app, l0, 'L0 unsaved draft')
                before = inspect(app)
                assert before['l0_draft'] == 'L0 unsaved draft', before
                assert before['fields'][2]['focus'] and before['fields'][2]['anchor'] != before['fields'][2]['cursor'], before
                observations = []
                for change in ('dark', 'violet', 'light', 'teal'):
                    switch(app, change)
                    after = inspect(app)
                    assert after['l0_ink'] == after['ink'], (change,after)
                    for key in ('script_heap', 'l0_heap', 'requests', 'timers', 'fields', 'l0_draft'):
                        assert after[key] == before[key], (change, key, before[key], after[key])
                    assert after['revision'] != before['revision'], change
                    app.wait_text('Count: 1')
                    app.wait_text('Shares the host theme and keeps its own state.')
                    app.capture(f'switch-{change}')
                    observations.append(after)
                    before = after
                # Undo still addresses the existing input history after restyling.
                app.request('/k', c='KeyZ', cmd=1, wait=1)
                undone = inspect(app)
                assert undone['fields'][2]['text'] == 'L0 draft', undone
                app.click_id('increment')
                app.wait_text('Count: 2')
                assert inspect(app)['requests'] == original['requests'] + 2
                app.click_id('catalog')
                app.wait_text('Built-in apps')
                switch(app, 'dark')
                app.wait_text('Built-in apps')
                app.capture('catalog-dark')
                app.click_id('browse_hub')
                app.wait_text('Log in to Matrix to browse mini apps')
                app.wait_text('My apps')
                switch(app, 'violet')
                app.wait_text('My apps')
                app.capture('library-dark-violet')
                app.click_id('back')
                app.wait_text('Built-in apps')
                app.click_id('launch')
                app.wait_text('Article studio')
                switch(app, 'light')
                app.wait_text('Article studio')
                app.capture('article-light-violet')
                app.click_id('article_continue')
                app.wait_text('Sign in to Rinx to authorize this app.')
                errors = [line for line in (app.output / 'native.log').read_text().splitlines()
                          if '[E]' in line or 'splash: splash:' in line or 'panicked at' in line]
                assert not errors, errors
                saved = json.loads((run / 'profile/appearance.json').read_text())
                assert saved == {'appearance': 'dark' if hosted else 'light', 'accent': 'violet'}, saved
                report['runs'].append({'hosted': hosted, 'state_preserved': True,
                    'startup_requests': original['requests'], 'timers': original['timers'],
                    'revisions': [o['revision'] for o in observations], 'evidence': str(app.output)})
            finally:
                app.stop()
            if not hosted:
                restarted = launch(run)
                try:
                    assert inspect(restarted)['selection'] == {'appearance':'light','accent':'violet'}
                    restarted.capture('persisted-light-violet')
                    report['standalone_restart_restores_selection'] = True
                finally:
                    restarted.stop()
        # Phone-width desktop instrumentation. This is not a device claim.
        app = launch(root / 'narrow', narrow=True)
        try:
            app.capture('phone-light')
            app.request('/m', k='scroll', x=250, y=650, dy=510, precise=1, wait=1)
            time.sleep(.2)
            before = {w['i']: w['r'] for w in app.snap() if w['i'] in ('script_app','l0_app')}
            assert before, 'No scrolled mini-app content'
            switch(app, 'dark')
            after = {w['i']: w['r'] for w in app.snap() if w['i'] in ('script_app','l0_app')}
            assert before == after, (before,after)
            state = inspect(app)
            assert state['l0_ink'] == state['ink'], state
            app.capture('phone-dark-scrolled')
            report['narrow_scroll_preserved'] = True
        finally:
            app.stop()
        report['passed'] = True
    finally:
        (root / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(dict(report, evidence=str(root))), flush=True)


if __name__ == '__main__':
    main()
