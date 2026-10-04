#!/usr/bin/env python3
"""Keep fixed interface palettes out of Rinx pages (ADR 0009).

Transparent shader values do not select a palette. Other fixed colors need a
local `theme-content:` explanation. Theme providers and test data own defaults;
they are not interface consumers. Also catch numeric Rust vec4 color mirrors.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
HEX = re.compile(r"(?<!\w)#x?([\da-fA-F]{3,8})\b")
VECTOR = re.compile(r"vec4\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\)")


def check(path):
    if path == ROOT / 'src/theme.rs' or path.parent == ROOT / 'src/theme':
        return []  # The single theme provider owns defaults and package examples.
    errors = []
    for line_no, line in enumerate(path.read_text().splitlines(), 1):
        if line.strip() == '#[cfg(test)]':
            break  # Rust unit-test modules follow production code in these files.
        code = line.split('//')[0]
        if 'theme-content:' in line:
            assert line.split('theme-content:')[1].strip(), (path, line_no)
            continue
        for match in HEX.finditer(code):
            digits = match[1]
            # Transparent fills and neutral translucent masks/scrims/shadows.
            if len(digits) in (4, 8):
                alpha = digits[-(len(digits)//4):]
                rgb = digits[:-len(alpha)]
                if int(alpha,16) == 0 or (set(rgb) == {'0'} and int(alpha,16) < (16**len(alpha)-1)):
                    continue
            errors.append(f'{path.relative_to(ROOT)}:{line_no}: explain {match[0]} or use a semantic role')
        for match in VECTOR.finditer(code):
            values = list(map(float, match.groups()))
            if values[3] == 0 or values == [-1]*4:  # transparent or framework sentinel
                continue
            errors.append(f'{path.relative_to(ROOT)}:{line_no}: use an instance theme value for {match[0]}')
    return errors


errors = [error for root in ('src','apps/article-editor/native')
          for path in (ROOT/root).rglob('*.rs') for error in check(path)]
if errors:
    print('\n'.join(errors), file=sys.stderr)
    sys.exit(1)
print('Theme literal guard passed')
