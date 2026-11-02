#!/usr/bin/env python3
"""Print WCAG contrast ratios for the themes and accents in src/ui/style/palette.rs.

Usage: scripts/contrast.py src/ui/style/palette.rs
Secondary text (faint) should stay at 4.5 or more on surfaces, white on an accent
fill too."""

import re,sys
src=open(sys.argv[1]).read()
def lum(h):
    r,g,b=[int(h[i:i+2],16)/255 for i in (0,2,4)]
    f=lambda c: c/12.92 if c<=0.03928 else ((c+0.055)/1.055)**2.4
    return 0.2126*f(r)+0.7152*f(g)+0.0722*f(b)
def cr(a,b):
    la,lb=lum(a),lum(b); return (max(la,lb)+0.05)/(min(la,lb)+0.05)
bases={}
for m in re.finditer(r'const (\w+): Base = Base \{(.*?)\};',src,re.S):
    d={k:'%s%s%s'%(x[2:],y[2:],z[2:]) for k,x,y,z in re.findall(r'(\w+): rgb\((0x\w\w), (0x\w\w), (0x\w\w)\)',m.group(2))}
    d['dark']='dark: true' in m.group(2); bases[m.group(1)]=d
acc={}
for m in re.finditer(r'Accent::(\w+) => \[(.*?)\],',src):
    cols=['%s%s%s'%(x[2:],y[2:],z[2:]) for x,y,z in re.findall(r'rgb\((0x\w\w), (0x\w\w), (0x\w\w)\)',m.group(2))]
    acc[m.group(1)]=cols
def mix(a,b,t):
    return ''.join('%02x'%round(int(a[i:i+2],16)+(int(b[i:i+2],16)-int(a[i:i+2],16))*t) for i in (0,2,4))
for n,b in bases.items():
    print(f"{n:9} faint/panel {cr(b['faint'],b['panel']):.2f} faint/surface {cr(b['faint'],b['surface']):.2f} dim/panel {cr(b['dim'],b['panel']):.2f} dim/surface {cr(b['dim'],b['surface']):.2f}")
    for a,c in acc.items():
        text,fill=(c[0],c[1]) if b['dark'] else (c[2],c[3])
        mine=mix(b['panel'],fill,0.22 if b['dark'] else 0.16)
        print(f"   {a:7} white/fill {cr('ffffff',fill):.2f} dark/fill {cr('121216',fill):.2f} accent/panel {cr(text,b['panel']):.2f} faint/mine {cr(b['faint'],mine):.2f} text/mine {cr(b['text'],mine):.2f}")
