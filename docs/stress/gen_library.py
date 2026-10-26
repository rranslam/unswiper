#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Synthetic stress library for docs/stress/README.md. No personal media.
import os, random, sys, datetime
from PIL import Image, ImageFilter
N=int(sys.argv[1]); out=sys.argv[2]; random.seed(7)
os.makedirs(out, exist_ok=True)
t=datetime.datetime(2019,1,1,9)
base=[Image.effect_noise((160,120),60+i*8).convert("RGB") for i in range(8)]
blurred=[b.filter(ImageFilter.GaussianBlur(4)) for b in base]
i=0
while i<N:
    t+=datetime.timedelta(hours=random.randint(30,200))   # new visit
    for k in range(random.randint(3,25)):
        if i>=N: break
        t+=datetime.timedelta(minutes=random.randint(1,40))
        img=(blurred if i%10==0 else base)[i%8]
        ex=Image.Exif(); ex[0x0132]=t.strftime("%Y:%m:%d %H:%M:%S")
        ex.get_ifd(0x8769)[0x9003]=t.strftime("%Y:%m:%d %H:%M:%S")
        d=os.path.join(out,f"{t.year}",f"{t.month:02d}"); os.makedirs(d,exist_ok=True)
        img.save(os.path.join(d,f"IMG_{i:06d}.jpg"),quality=80,exif=ex)
        i+=1
