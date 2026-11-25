"""Run inside an isolated desktop session with WebKitWebDriver on port 4444."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from urllib.request import Request, urlopen
from urllib.error import HTTPError

base = 'http://127.0.0.1:4444'
app = os.path.abspath(sys.argv[1])
fixtures = Path(sys.argv[2]).resolve()


def call(method, path, data=None):
    req = Request(base + path, data=None if data is None else json.dumps(data).encode(),
                  method=method, headers={'Content-Type': 'application/json'})
    try:
        with urlopen(req, timeout=35) as res:
            return json.load(res)['value']
    except HTTPError as e:
        raise RuntimeError(e.read().decode()) from e


def wait(fn, seconds=15):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = fn()
        if value:
            return value
        time.sleep(.08)
    raise RuntimeError('Timed out waiting for the viewer')


session = call('POST', '/session', {'capabilities': {'alwaysMatch': {
    'webkitgtk:browserOptions': {'binary': app, 'args': [str(fixtures / 'standard.png')]}}}})
session_path = '/session/' + session['sessionId']


def js(script):
    return call('POST', session_path+'/execute/sync', {'script':script, 'args':[]})


def ready(filename):
    def check():
        err = js("return document.querySelector('#emptyError:not([hidden]) #errorText')?.textContent || ''")
        if err:
            raise RuntimeError(err)
        return js("return document.querySelector('#filename')?.textContent === "+json.dumps(filename)+
                  " && !document.querySelector('#canvas').hidden && document.querySelector('#viewport').getAttribute('aria-busy') === 'false'")
    wait(check)


try:
    ready('standard.png')
    assert js("return !document.getElementById('crop').disabled")
    print('Opened still image from command line', flush=True)
    js("document.getElementById('crop').click()")
    wait(lambda: js("return !document.getElementById('cropBar').hidden"))
    js("document.getElementById('cancelCrop').click()")
    js("document.getElementById('settings').click()")
    wait(lambda: js("return document.getElementById('settingsDialog').open"))
    if os.environ.get('PVA_TEST_DEFAULT') == '1':
        # Only enable this in a disposable user profile.
        js("document.getElementById('setDefault').click()")
        wait(lambda: subprocess.check_output(['xdg-mime', 'query', 'default', 'image/png'], text=True).strip() == 'com.photovieweradvanced.desktop')
        print('Registered the viewer for PNG in the disposable profile', flush=True)
    js("document.getElementById('closeSettings').click()")
    for name in ('infinite.gif', 'infinite.webp', 'timing-disposal.apng', 'animated-avif.avif', 'Unicode 空格 [1] & café #%.png'):
        subprocess.run([app, str(fixtures / name)], check=True, timeout=10)
        ready(name)
        if name.startswith('Unicode'):
            break
        frame = js("return document.getElementById('frameLabel').textContent")
        assert '/' in frame, (name, frame)
        if name.startswith('infinite'):
            wait(lambda: js("return document.getElementById('frameLabel').textContent") != frame)
            js("document.getElementById('playPause').click()")
            wait(lambda: js("return document.getElementById('playPause').title.startsWith('Play')"))
        print('Rendered', name, frame, flush=True)
    print('Opened Unicode image from a second process', flush=True)
finally:
    call('DELETE',session_path)
