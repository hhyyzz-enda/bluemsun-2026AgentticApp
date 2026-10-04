#!/usr/bin/env python3
"""Start the actual release app with isolated light/dark customer-theme profiles.

Build: cargo build --release --locked
Run from the repository root. No account, login or room sends are performed.
"""
import sys,os,subprocess,time,socket,json,hashlib,uuid
from pathlib import Path
from PIL import Image
from native_probe import NativeApp


def main():
    root=Path('target/theme-release-validation')/uuid.uuid4().hex;root.mkdir(parents=True)
    report={'passed':False,'binary_sha256':hashlib.sha256(Path('target/release/rinx').read_bytes()).hexdigest(),'runs':[]}
    try:
     for mode in ['light','dark']:
      run=(root/mode).resolve();profile=run/'profile';profile.mkdir(parents=True)
      package=json.loads(Path('examples/themes/ocean-violet.octotheme').read_text())
      state={'current':{'selection':{'appearance':mode,'accent':'teal'},'package':package,'follow_system':False},'previous':None}
      (profile/'theme-state.json').write_text(json.dumps(state))
      with socket.socket() as s:s.bind(('127.0.0.1',0));port=s.getsockname()[1]
      app=NativeApp(run,port,auto_login=False);app.output.mkdir(parents=True);app.log=(app.output/'native.log').open('w')
      app.process=subprocess.Popen(['target/release/rinx'],env=dict(os.environ,MAKEPAD_REMOTE=str(port),MAKEPAD_HIDE_WINDOWS='1',MAKEPAD_NO_FOCUS='1',RINX_DATA_DIR=str(profile)),stdout=app.log,stderr=subprocess.STDOUT)
      try:
       for _ in range(120):
        if app.process.poll() is not None:raise RuntimeError('Rinx exited before its first frame')
        try:
         status=app.request('/s')
         if status['w']:break
        except OSError:pass
        time.sleep(.25)
       app.wait_text('Sign in')
       path=app.capture('release-startup')
       (run/'widgets.json').write_text(json.dumps(app.snap(),indent=2))
       pixels=Image.open(path).convert('RGB');color=max(pixels.getcolors(pixels.width*pixels.height),key=lambda row:row[0])[1]
       assert (sum(color)<200) == (mode=='dark'), (mode,color)
       log=(app.output/'native.log').read_text()
       assert '[E]' not in log and 'panicked at' not in log and 'Splash script errors' not in log
       assert json.loads((profile/'theme-state.json').read_text())['current']['package']['id']=='ocean-violet'
       report['runs'].append({'mode':mode,'dominant_color':color,'profile_theme_retained':True,'evidence':str(run)})
      finally:app.stop()
     report['passed']=True
    finally:
     (root/'report.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'evidence':str(root),**report}))


if __name__ == "__main__":
    main()
