#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Synthetic stress library for docs/stress/README.md (Linux and Mac). No personal media.

Python 3 standard library only. Writes N tiny JPEGs (64x48 noise, every tenth one blurred)
with EXIF capture dates in bursts of 3-25 photos (about 1,400 visits for 20,000 photos),
in YEAR/MONTH folders. With --burst M it also adds one visit of M photos taken on one day
(hard links of one file, so it costs no space).

    python3 gen_library.py 20000 ~/StressLib --burst 6000
"""
import argparse
import base64
import datetime
import os
import random
import struct

SHARP = base64.b64decode(
    "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAoHBwgHBgoICAgLCgoLDhgQDg0NDh0VFhEYIx8lJCIf"
    "IiEmKzcvJik0KSEiMEExNDk7Pj4+JS5ESUM8SDc9Pjv/2wBDAQoLCw4NDhwQEBw7KCIoOzs7Ozs7"
    "Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozv/wAARCAAwAEADASIA"
    "AhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQA"
    "AAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3"
    "ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWm"
    "p6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEA"
    "AwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSEx"
    "BhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElK"
    "U1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3"
    "uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwCGLyGu"
    "VS92xXEUGLiR4t8kiszKWV5QS5IkVVI3ZDnAUFdtqNriewis5bSCNJZxKytJ5KRO7OYjJ5f3d3Qb"
    "enPfAWI3mrJ/pE2oq8Mylb2Ry9wikNvdQCw8tgu0kYUKvHVSBJp1sLZIZbmExW8hiDbYCiyMJEKh"
    "Wyrop2DaoTcChOB0IBRuBY3GnWLWtm8NpfXaHzVfzSxMzNhVPLOjOAE2kkZyuPmG1pdzYtJOhuRP"
    "qO12EDRcmN5vkZljGCh/dkhcEhV6BipoW+qRzeIFnuLy6uCZT5imFRDLJymTJj737tFHKkklAcMT"
    "TYJZbG2sbhLmV/NKzpLNKSJWjAcuYiu7LMd2VOSMs3zHbQBfkKXxMX2eRZ1t5JWRLaRDcTEH94ZA"
    "oXChcgKpOAeS2FqnqN1qFzHBdSJdWxwz27SxqVEJiYMHRdrIAu5Tt5OcgDy9osQW0GlXXkSx2DyR"
    "u7QiTbDsiDMHYzg7VPm8MnYsMjiqttaXFyuoXii+gb5w6vJJM8m4R7AXVSJQSoAYPkbVAAOdwA+4"
    "0m/ut2+8bVriJW8z7O0cs4LAKoKlAH2kOpz8xMe7IC7RFDrNm+nWVvP5Xli8bzbq2lZkiiAwRuHJ"
    "JPHBAAC42llqRr6SaW4t4ReLb3d24md7YhbvMgKyYRPm3KCQqqGJzgkM22/O7aLcCBryO8uZZJVl"
    "gnuFy0Yyu2MblwSyKTwN2CMsAAoBVlj067kd10+fWJorhnlKykIplKJiUggZCqS2W/5YjdtYkql3"
    "cta2ttFp0kkroTcQb5nZl8vYQCu4lUZ5GztAyAVyAARUk0+SPw0kTjMDnbFOYVHl7FeQArM218li"
    "wIbjDKT8op0bJYW91DPql1ZJaAi4jFus6zNhtjSElc/KrEJtGAqseF3EA09WkvnmNnYSxx2/kif7"
    "MpGyXDbVRQkeBn95tTqX55GSKdvoR1CcwxwW8S+a4hfLIZFO5GJBIDEyb2K43KDgbcbQ+2hubXWr"
    "eznMEgnQMwWKNHL+YAwkMa4QrvdVIbblQG7LVZ7K7ttRjunWcebmOR8L5ryu5zI8KI6u2UKYyQxR"
    "c/eFAFeGexl1JjcaajKWit5IbmYiWEusjBY1ABIyzHYPmZtvC8MyTT2upQ3PlfZrg3cnlrJcqFJk"
    "8sKqNsVtjESMVz/cVc5GxJoJm0i4mtfIis3YR+aCAZ1wQ7hWQnzMkxvkFSFAxjcHqWxmW2tFmZDA"
    "YYGT7XASgMyuQFwirnKOAD82wvgfN94AzDqiSwRWGk2kllO0wluohD9pkkwd4YAoA6ErHg4JzwQR"
    "szrzfZ5bm0S5sYmFyyQxpNK7vB5e5GRRwzEEMpDEZC9cfNVSxj0u4T7VdXawyyNmOe3mdAJFkZGf"
    "nLeXuK4YbgDk5UgqWwtDYPNAbnSrudiPKVfMEcjqGVXdVyAxDooJBAIwDu3YANITxrKtsLiOys4E"
    "wxljaDzc7pCGk3D5GYbyR975MEAjOHaahc6lezXTQSQQXxVXV5gpkCrI5AfblU2KnYABTwQSrdFp"
    "92v2YXf25HhuJv3V2kOLjzGDLG4DBtzOJChBXGMk4LZasiSHT5rG4mWeJIGjhlEBdm2sN4dSditl"
    "o03B8nKHHOVAHFJDIbNTbhiGlmt4RHHMUZSQjxbgidASR/eXGcA1WVLuRZI1g1CSB1SIyw3ckLbm"
    "8tPMLuN2fMCqo2HC5UhQp3TbLfTozdX1vGbiJWRxak7pSNxZiA2wthWPGQVVlw4UBZII/sOnW8kd"
    "yHjELSQyXUkflLlikjLgsqqA6qzYyT82QCdwBnWl5BJZXi2+n3UM1u6x2sTSKx8wNtjyVH8f3c8D"
    "LH5ssBVqaG+R7O1uUeCwjMQMeHXe6yEMC20IcKFL4+UdcqAEMum2iXQsYbJ7mf5Q0kETblAZJUZH"
    "JYbfmWYcNgbxtzkyVQghW2jkikhG69XEUk022KaVpXYoSnO9cyAyOwOE+6RQBOWspI7zKy2zwRnd"
    "DbJvaFdxBMgOfmR5DvxjoNrAx1DcpbWdvbASiaxCNJEsdwUkjkWNTIqyg5aNlwTyDtlBGArlb93d"
    "RyNJP5cEKSzKYYxlBMgDMk7gMclY8ksNp3qFPUinSTRabcXUltCLGaeCTO88RT75AjlAnzkbXOQv"
    "RWGdoKqAf//Z"
)
BLURRED = base64.b64decode(
    "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAoHBwgHBgoICAgLCgoLDhgQDg0NDh0VFhEYIx8lJCIf"
    "IiEmKzcvJik0KSEiMEExNDk7Pj4+JS5ESUM8SDc9Pjv/2wBDAQoLCw4NDhwQEBw7KCIoOzs7Ozs7"
    "Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozs7Ozv/wAARCAAwAEADASIA"
    "AhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQA"
    "AAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3"
    "ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWm"
    "p6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEA"
    "AwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSEx"
    "BhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElK"
    "U1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3"
    "uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwCPZzk1"
    "Mv3cVC27NTxqduaAIHj+bNXbVwi1Wcc0K+KALrvk8VWkck4oEnFMY85oAa0WealhcqMU1JNxxUhw"
    "vNADWZCacrjoKriJgtIpwcUAWJAGHFQlMVIvNMkBFAEW7BodzimnrT9o20AQLMVercb7xzUAiGc0"
    "u7YeKALcvC8CqQ3GSryMJFpPJGaAGKeKY+alK7aCBtoArCnAZqZIg3SmkbGwaAG7OKieM1aBBpk2"
    "FFAH/9k="
)


def exif_segment(when):
    """APP1 Exif segment with DateTimeOriginal (and DateTime) set to `when`."""
    stamp = when.strftime("%Y:%m:%d %H:%M:%S").encode() + b"\0"  # 20 bytes
    # TIFF header (little endian), IFD0 at 8: DateTime + ExifIFD pointer; Exif IFD: DTO.
    ifd0_off = 8
    ifd0_len = 2 + 2 * 12 + 4
    exif_off = ifd0_off + ifd0_len
    exif_len = 2 + 1 * 12 + 4
    data_off = exif_off + exif_len
    dt_off, dto_off = data_off, data_off + 20
    t = b"II*\0" + struct.pack("<I", ifd0_off)
    t += struct.pack("<H", 2)
    t += struct.pack("<HHII", 0x0132, 2, 20, dt_off)
    t += struct.pack("<HHII", 0x8769, 4, 1, exif_off)
    t += struct.pack("<I", 0)
    t += struct.pack("<H", 1)
    t += struct.pack("<HHII", 0x9003, 2, 20, dto_off)
    t += struct.pack("<I", 0)
    t += stamp + stamp
    body = b"Exif\0\0" + t
    return b"\xff\xe1" + struct.pack(">H", len(body) + 2) + body


def jpeg(base, when):
    assert base[:2] == b"\xff\xd8"
    return base[:2] + exif_segment(when) + base[2:]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("count", type=int)
    ap.add_argument("folder")
    ap.add_argument("--burst", type=int, default=0, help="add one visit of this many photos")
    ap.add_argument("--seed", type=int, default=7)
    a = ap.parse_args()
    random.seed(a.seed)
    t = datetime.datetime(2019, 1, 1, 9)
    i = 0
    while i < a.count:
        t += datetime.timedelta(hours=random.randint(30, 200))  # a new visit
        for _ in range(random.randint(3, 25)):
            if i >= a.count:
                break
            t += datetime.timedelta(minutes=random.randint(1, 40))
            d = os.path.join(a.folder, str(t.year), "%02d" % t.month)
            os.makedirs(d, exist_ok=True)
            with open(os.path.join(d, "IMG_%06d.jpg" % i), "wb") as f:
                f.write(jpeg(BLURRED if i % 10 == 0 else SHARP, t))
            i += 1
    if a.burst:
        d = os.path.join(a.folder, "0000burst")
        os.makedirs(d, exist_ok=True)
        first = os.path.join(d, "B_000000.jpg")
        with open(first, "wb") as f:
            f.write(jpeg(SHARP, datetime.datetime(2018, 6, 1, 10)))
        for k in range(1, a.burst):
            os.link(first, os.path.join(d, "B_%06d.jpg" % k))
    print("wrote %d photos%s in %s" % (a.count, " + %d in one visit" % a.burst if a.burst else "", a.folder))


if __name__ == "__main__":
    main()
