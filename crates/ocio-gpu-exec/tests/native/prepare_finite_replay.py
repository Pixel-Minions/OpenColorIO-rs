import hashlib
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1]).resolve()
compiled = [(p, p.read_text()) for p in (root / 'compiled').glob('*.glsl')]
rows = {'original': [], 'version400': [], 'precise': []}
variants = []
for folder in sorted(root.iterdir()):
    metadata = folder / 'observations.json'
    if not folder.is_dir() or not metadata.exists():
        continue
    data = json.loads(metadata.read_text())
    assert data['all_input_finite'] and data['port_wheel_gpu_exact']
    assert not data['gpu_metadata']['shader']['uniforms'], 'static fixtures only'
    assert (folder / 'port.bin').read_bytes() == (folder / 'wheel.bin').read_bytes()
    component = int(re.search(r'index_diff: Some\((\d+)\)', data['comparison']).group(1))
    pixel = component // 4
    for kind in ('input', 'cpu', 'port'):
        raw = (folder / (kind + '.bin')).read_bytes()
        point = raw[pixel * 16:pixel * 16 + 16]
        assert len(point) == 16
        (folder / ('point-' + kind + '.bin')).write_bytes(point)
    code = (folder / 'ocio.glsl').read_text()
    matches = [(p, text) for p, text in compiled if code in text]
    assert matches, folder
    assert len({text for _, text in matches}) == 1, 'one exact compiled source'
    original_path, original = matches[0]
    version400, count = re.subn(r'\A#version[ \t]+\d+(?:[ \t]+\w+)?', '#version 400 compatibility', original, count=1)
    assert count == 1
    qualified, declarations = re.subn(r'(?m)^([ \t]*)(float|vec[234]|mat[234])[ \t]+([A-Za-z_]\w*)([ \t]*)(?=[=;\[])', r'\1precise \2 \3\4', version400)
    assert declarations > 0
    version_path = folder / 'version400.glsl'
    precise_path = folder / 'precision-qualified.glsl'
    version_path.write_text(version400)
    precise_path.write_text(qualified)
    variants.append({'test': folder.name, 'original_version': original.splitlines()[0], 'precise_local_declarations': declarations, 'original_sha256': hashlib.sha256(original.encode()).hexdigest(), 'version400_sha256': hashlib.sha256(version400.encode()).hexdigest(), 'precise_sha256': hashlib.sha256(qualified.encode()).hexdigest(), 'diagnostic_only': True})
    resource_rows = []
    for texture in data['textures3d'] + data['textures']:
        dimensions = 3 if 'edge' in texture else {'TEXTURE_1D': 1, 'TEXTURE_2D': 2}[texture['dimensions']]
        channels = 3 if dimensions == 3 else {'TEXTURE_RED_CHANNEL': 1, 'TEXTURE_RGB_CHANNEL': 3}[texture['channel']]
        values = [texture['sampler'], dimensions, texture.get('width', texture.get('edge')), 1 if dimensions == 1 else texture.get('height', texture.get('edge')), texture.get('edge', 0), channels, 0x2600 if texture['interpolation'] == 'INTERP_NEAREST' else 0x2601, folder / texture['file']]
        resource_rows.append(' '.join(map(str, values)))
    for kind, shader_path in [('original', original_path), ('version400', version_path), ('precise', precise_path)]:
        rows[kind].append(' '.join(map(str, [folder, shader_path, len(resource_rows)])) + '\n' + '\n'.join(resource_rows))
assert all(len(values) == 5 for values in rows.values())
assert all(not any(c.isspace() for c in str(p)) for p in root.rglob('*.bin'))
for kind, values in rows.items():
    filename = 'native-manifest.txt' if kind == 'original' else 'native-' + kind + '-manifest.txt'
    (root / filename).write_text('5\n' + '\n'.join(values) + '\n')
(root / 'precision-variant-metadata.json').write_text(json.dumps(variants, indent=2) + '\n')
print('Five public fixtures mapped; original GLSL120 preserved; separate version400 and precise-local diagnostics prepared')
