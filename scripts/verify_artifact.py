#!/usr/bin/env python3
"""Check the deliverable, not an unrelocated intermediate jar."""
import hashlib
import json
from datetime import datetime
from zoneinfo import ZoneInfo
import pathlib
import struct
import xml.etree.ElementTree as ET
import zipfile

root = pathlib.Path(__file__).resolve().parents[1]
jar = root / 'dist/PurtmarsArea.jar'
with zipfile.ZipFile(jar) as archive:
    assert archive.testzip() is None, 'Corrupted ZIP'
    names = set(archive.namelist())
    plugin = archive.read('plugin.yml').decode()
    assert "api-version: '26.2'" in plugin
    assert 'folia-supported: false' in plugin
    assert 'depend:' not in plugin, 'Unexpected standalone TabooLib dependency'
    assert 'main: ink.ptms.purtmarsarea.taboolib.platform.BukkitPlugin' in plugin
    assert 'ink/ptms/purtmarsarea/taboolib/platform/BukkitPlugin.class' in names
    assert not any(n.startswith(('me/skymc/', 'taboolib/', 'org/bukkit/', 'net/minecraft/')) for n in names)
    notice = archive.read('META-INF/licenses/TabooLib-LICENSE.txt').decode()
    assert 'Copyright (c) 2018 Bkm016' in notice and 'Permission is hereby granted' in notice
    props = archive.read('META-INF/taboolib/version.properties').decode()
    assert 'taboolib=6.3.0-0e3a911' in props
    assert 'kotlin=2.3.20' in props
    for name in names:
        if name.endswith('.class'):
            major = struct.unpack('>H', archive.read(name)[6:8])[0]
            assert major <= 69, f'{name} requires a newer Java than 25'
    assert struct.unpack('>H', archive.read('ink/ptms/purtmarsarea/PurtmarsArea.class')[6:8])[0] == 69

count = failed = skipped = 0
suites = []
for report in (root / 'build/test-results/test').glob('TEST-*.xml'):
    suite = ET.parse(report).getroot()
    suites.append({'suite': suite.attrib['name'], 'tests': int(suite.attrib['tests']), 'failures': int(suite.attrib['failures']), 'errors': int(suite.attrib['errors']), 'skipped': int(suite.attrib['skipped'])})
    count += int(suite.attrib['tests'])
    failed += int(suite.attrib['failures']) + int(suite.attrib['errors'])
    skipped += int(suite.attrib['skipped'])
assert count > 0 and failed == skipped == 0, (count, failed, skipped)
digest = hashlib.sha256(jar.read_bytes()).hexdigest()
(root / 'dist/SHA256SUMS').write_text(f'{digest}  PurtmarsArea.jar\n')
print(f'PASS: {count} tests; Java 25; Paper 26.2; TabooLib 6.3.0-0e3a911')
print(f'SHA256 {digest}')

report = {'date': datetime.now(ZoneInfo('Asia/Shanghai')).date().isoformat(), 'target': 'Paper 26.2', 'java_class_major': 69, 'taboolib': '6.3.0-0e3a911', 'gradle_plugin': '2.0.39', 'artifact': 'dist/PurtmarsArea.jar', 'sha256': digest, 'suites': sorted(suites, key=lambda s: s['suite']), 'live_server_tested': False, 'real_player_tested': False}
(root / 'dist/verification.json').write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
