# Turns the installer artwork (src-tauri/installer/art/*.png) into the 24-bit BMPs NSIS needs:
#   header.bmp 500x57 and header2x.bmp 1000x114 (full width across the top of every page; header2x on 200% screens)
#   sidebar.bmp 164x314 (Welcome and Finish pages), uninstaller-header.bmp 150x57 (the uninstaller's corner)
# Run after changing the art: python docs/installer_art.py
import os
from PIL import Image

inst = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'src-tauri', 'installer')
BG = (7, 18, 14)

def bmp(src, size, out):
    im = Image.open(os.path.join(inst, 'art', src)).convert('RGBA')
    if im.size != size:
        im = im.resize(size, Image.LANCZOS)
    flat = Image.new('RGB', size, BG)
    flat.paste(im, mask=im.split()[3])
    flat.save(os.path.join(inst, out))
    print(out, size)

bmp('header@2x.png', (1000, 114), 'header2x.bmp')
bmp('header@2x.png', (500, 57), 'header.bmp')
bmp('sidebar.png', (164, 314), 'sidebar.bmp')
bmp('header-small.png', (150, 57), 'uninstaller-header.bmp')
