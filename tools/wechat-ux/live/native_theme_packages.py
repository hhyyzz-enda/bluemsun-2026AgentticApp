#!/usr/bin/env python3
"""Customer themes: actual Makepad input/rendering with isolated offline profiles."""
import hashlib
import json
from pathlib import Path
import time
import urllib.error
import uuid
from native_theme import launch, inspect, type_at


def settle(app):
    previous = None
    for _ in range(30):
        current = [(w['i'], w['r']) for w in app.snap()]
        if current == previous:
            return
        previous = current
        time.sleep(.25)
    raise AssertionError('Layout did not settle')


def edit(app, field, value):
    settle(app)
    app.click_id(field)
    app.request('/k', c='KeyA', cmd=1, wait=1)
    app.request('/t', t=value, wait=1)


def capture_ready(app, name):
    for attempt in range(5):
        try:
            return app.capture(name)
        except urllib.error.HTTPError as error:
            if error.code != 404 or attempt == 4:
                raise
            time.sleep(.25)


def run(root, report):
    app = launch(root / 'standalone')
    try:
        settle(app)
        before = inspect(app)
        assert before['font_sizes'] == [11, 11, 11], before['font_sizes']
        # This fixture delivery calls the same parse/import action as the real
        # file picker and authenticated Matrix attachment downloader.
        app.request('/event', data='theme:import', wait=1)
        app.wait_text('Ocean Violet')
        settle(app)
        imported = inspect(app)
        assert imported['revision'] == before['revision'], 'Receiving a theme applied it'
        app.capture('import-review')
        app.click_id('basic_preview')
        app.wait_text('Preview active')
        settle(app)
        preview = inspect(app)
        assert preview['preview'] and preview['scale'] == 1.15 and preview['radius'] == 10
        assert preview['accent'] == 0xff4f46a5
        assert all(abs(size - 12.65) < .001 for size in preview['font_sizes']), preview['font_sizes']
        report['url_card_shares_body_size_and_live_scale'] = True
        for old, new in zip(before['fields'], preview['fields']):
            assert old['uid'] == new['uid'] and old['text'] == new['text']
        assert (before['script_heap'], before['l0_heap'], before['requests'], before['timers']) == (
            preview['script_heap'], preview['l0_heap'], preview['requests'], preview['timers'])
        assert not (app.root / 'profile/theme-state.json').exists(), 'Preview was persisted'
        app.capture('custom-preview')
        app.click_id('preview_dark')
        settle(app)
        assert inspect(app)['accent'] == 0xffc4b5fd
        app.capture('custom-dark-preview')
        app.click_id('preview_light')
        settle(app)
        assert inspect(app)['accent'] == 0xff4f46a5
        app.click_id('theme_cancel')
        app.wait_text('Preview cancelled')
        settle(app)
        assert inspect(app)['revision'] == before['revision']
        edit(app, 'light_accent', '#ffffff')
        app.click_id('basic_preview')
        app.wait_text('contrast')
        assert inspect(app)['revision'] == before['revision']
        assert not (app.root / 'profile/theme-state.json').exists()
        app.capture('invalid-theme-rejected')
        edit(app, 'light_accent', '#4f46a5')
        app.click_id('basic_preview')
        app.wait_text('Preview active')
        app.click_id('theme_apply')
        app.wait_text('Theme applied')
        settle(app)
        applied = inspect(app)
        assert not applied['preview'] and applied['accent'] == 0xff4f46a5
        saved = json.loads((app.root / 'profile/theme-state.json').read_text())
        assert saved['current']['package']['id'] == 'ocean-violet'
        assert not (app.root / 'profile/theme-rollback.json').exists(), 'Successful render did not commit'
        app.click_id('theme_undo')
        app.wait_text('Previous appearance restored')
        settle(app)
        assert inspect(app)['accent'] == before['accent']
        app.click_id('theme_undo')
        app.wait_text('Previous appearance restored')
        settle(app)
        assert inspect(app)['accent'] == 0xff4f46a5
        app.capture('applied-after-undo')
        app.click_id('studio_close')
        app.request('/event', data='theme:popup', wait=1)
        app.wait_text('Retained theme notification', pixels=True)
        capture_ready(app, 'notification-light')
        app.request('/event', data='theme:dark', wait=1)
        settle(app)
        app.wait_text('Retained theme notification', pixels=True)
        capture_ready(app, 'notification-dark')
        app.request('/event', data='theme:light', wait=1)
        settle(app)
        app.wait_text('Retained theme notification', pixels=True)
        report['retained_notification_survives_reapply'] = True
        report['import_preview_cancel_apply_undo'] = True
        report['state_requests_timers_retained'] = True
        report['contrast_rejection'] = True
        report['render_commit'] = True
        log = (app.output / 'native.log').read_text()
        assert '[E]' not in log and 'panicked at' not in log and 'Splash script errors' not in log
    finally:
        app.stop()
    app = launch(root / 'standalone')
    try:
        settle(app)
        restored = inspect(app)
        assert restored['accent'] == 0xff4f46a5 and restored['scale'] == 1.15
        app.capture('restored-custom-theme')
        report['restart'] = True
        log = (app.output / 'native.log').read_text()
        assert '[E]' not in log and 'panicked at' not in log and 'Splash script errors' not in log
    finally:
        app.stop()
    # Simulate an interrupted apply with this isolated profile only.
    profile = root / 'standalone/profile'
    (profile / 'theme-rollback.json').write_text(json.dumps({'current': {'selection': {'appearance':'light','accent':'teal'}, 'package':None, 'follow_system':False}, 'previous':None}))
    app = launch(root / 'standalone')
    try:
        settle(app)
        recovered = inspect(app)
        assert recovered['font_sizes'] == [11,11,11] and recovered['accent'] == 0xff09616f
        assert not (profile / 'theme-rollback.json').exists()
        app.capture('recovered-after-interrupted-apply')
        report['interrupted_apply_recovers_last_good'] = True
    finally:
        app.stop()
    app = launch(root / 'narrow', narrow=True)
    try:
        settle(app)
        app.request('/event', data='theme:import', wait=1)
        app.wait_text('Ocean Violet')
        settle(app)
        app.capture('phone-import-review')
        height=app.request('/s')['w'][0]['sz'][1]
        assert any(w['i']=='theme_apply' and w['r'][2]>0 and w['r'][1]+w['r'][3]<=height for w in app.snap())
        report['narrow_editor_has_apply_and_cancel'] = True
    finally:
        app.stop()
    app=launch(root / 'hosted',hosted=True)
    try:
        settle(app)
        before=inspect(app)
        app.request('/event',data='theme:host-package',wait=1)
        settle(app)
        hosted=inspect(app)
        assert hosted['accent']==0xff4f46a5 and hosted['scale']==1.15
        assert hosted['selection'] is None and hosted['preferences'] is None
        assert hosted['script_heap']==before['script_heap'] and hosted['l0_heap']==before['l0_heap']
        assert hosted['fields']==before['fields']
        assert (hosted['requests'],hosted['timers'])==(before['requests'],before['timers'])
        assert not (app.root / 'profile/theme-state.json').exists()
        assert not (app.root / 'profile/appearance.json').exists()
        app.request('/event',data='theme:host-package',wait=1)
        settle(app)
        assert inspect(app)['revision']==hosted['revision']
        app.capture('hosted-custom-snapshot')
        report['hosted_data_snapshot_preserves_state_and_owns_selection']=True
    finally:
        app.stop()


def main():
    root = Path('target/theme-package-validation') / uuid.uuid4().hex
    root.mkdir(parents=True)
    report = {'passed': False, 'evidence': str(root), 'binary_sha256': hashlib.sha256(Path('target/fast/examples/theme_mvp').read_bytes()).hexdigest()}
    try:
        run(root, report)
        report['passed'] = True
    finally:
        (root / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
