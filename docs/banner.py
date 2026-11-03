# Renders docs/banner.html to docs/banner.png for a release: python docs/banner.py 0.4.2
import os, subprocess, sys

here = os.path.dirname(os.path.abspath(__file__))
chrome = r'C:\Program Files\Google\Chrome\Application\chrome.exe'
page = 'file:///' + os.path.join(here, 'banner.html').replace(os.sep, '/') + '#' + sys.argv[1]
out = os.path.join(here, 'banner.png')
subprocess.run([chrome, '--headless=new', '--disable-gpu', '--allow-file-access-from-files', '--hide-scrollbars', '--force-device-scale-factor=1',
                '--window-size=1280,640', '--virtual-time-budget=2000', '--screenshot=' + out, page], capture_output=True)
print(out, os.path.getsize(out))
