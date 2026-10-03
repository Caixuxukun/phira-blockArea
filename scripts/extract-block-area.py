"""Inspect the user's Phigros 4.0.1 APK without extracting its music/charts.

Requires UnityPy==1.25.4. Output is local analysis data, not a Unity project.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import zipfile

import lz4.block
import UnityPy


def extract(apk, output, native):
    output.mkdir(parents=True, exist_ok=True)
    with apk.open('rb') as file:
        digest = hashlib.file_digest(file, 'sha256').hexdigest()
    manifest = {'apk': apk.name, 'sha256': digest, 'objects': []}
    with zipfile.ZipFile(apk) as archive:
        env = UnityPy.load(archive.read('assets/bin/Data/data.unity3d'))
        if native:
            for entry in ('lib/arm64-v8a/libil2cpp.so', 'assets/bin/Data/Managed/Metadata/global-metadata.dat'):
                (output / entry.rsplit('/', 1)[-1]).write_bytes(archive.read(entry))
    for obj in env.objects:
        if obj.assets_file.name != 'sharedassets12.assets':
            continue
        if obj.type.name == 'Texture2D' and obj.path_id in (15, 17, 30):
            texture = obj.read()
            texture.image.save(output / f'{obj.path_id}_{texture.m_Name}.png')
            manifest['objects'].append({'id': obj.path_id, 'type': 'Texture2D', 'name': texture.m_Name})
        elif obj.type.name in ('Shader', 'Material'):
            tree = obj.read_typetree()
            name = tree.get('m_ParsedForm', {}).get('m_Name', tree.get('m_Name', ''))
            if not any(key in name for key in ('Block', 'EdgeMask', 'GlowMask', 'TouchEffect')):
                continue
            stem = re.sub(r'[^\w.-]', '_', f'{obj.assets_file.name}_{obj.path_id}_{name}')
            (output / f'{stem}.json').write_text(json.dumps(tree, indent=2, default=str), encoding='utf8')
            manifest['objects'].append({'id': obj.path_id, 'type': obj.type.name, 'name': name})
            if obj.type.name == 'Shader':
                # This APK's shaders contain a single GLES3 platform blob.
                if tree['platforms'] != [9]:
                    raise ValueError(f'Unexpected shader platform layout for {name}')
                blob = lz4.block.decompress(bytes(tree['compressedBlob']), uncompressed_size=tree['decompressedLengths'][0][0])
                programs = re.findall(rb'[\x09\x0a\x0d\x20-\x7e]{80,}', blob)
                (output / f'{stem}.glsl').write_text('\n'.join(p.decode('utf8') for p in programs), encoding='utf8')
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf8')
    print(f'Extracted {len(manifest["objects"])} objects. APK SHA256: {digest}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('apk', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--native', action='store_true', help='Also extract ARM64 IL2CPP and metadata for method inspection')
    args = parser.parse_args()
    extract(args.apk, args.output, args.native)
