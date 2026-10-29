import json,pathlib,sys
root=pathlib.Path(sys.argv[1]).resolve()
compiled=[p for p in (root/'compiled').glob('*.glsl')]
rows=[]
for folder in sorted(root.iterdir()):
 if not folder.is_dir() or folder.name=='compiled':continue
 p=folder/'observations.json'
 if not p.exists():continue
 data=json.loads(p.read_text())
 assert data['all_input_finite'] and data['port_wheel_gpu_exact']
 assert not data['gpu_metadata']['shader']['uniforms'],'static fixture only'
 assert (folder/'port.bin').read_bytes()==(folder/'wheel.bin').read_bytes()
 code=(folder/'ocio.glsl').read_text()
 matches=[p for p in compiled if code in p.read_text()]
 assert matches,folder
 assert len({p.read_text() for p in matches})==1,'one exact compiled source per fixture'
 resource_rows=[]
 for r in data['textures3d']+data['textures']:
  dim=3 if 'edge' in r else {'TEXTURE_1D':1,'TEXTURE_2D':2}[r['dimensions']]
  channel=3 if dim==3 else {'TEXTURE_RED_CHANNEL':1,'TEXTURE_RGB_CHANNEL':3}[r['channel']]
  resource_rows.append(' '.join(map(str,[r['sampler'],dim,r.get('width',r.get('edge')),1 if dim==1 else r.get('height',r.get('edge')),r.get('edge',0),channel,0x2600 if r['interpolation']=='INTERP_NEAREST' else 0x2601,folder/r['file']])))
 rows.append(' '.join(map(str,[folder,matches[0],len(resource_rows)]))+'\n'+'\n'.join(resource_rows))
assert len(rows)==5,'all five captures required'
assert all(not any(c.isspace() for c in str(p)) for p in root.rglob('*.bin'))
(root/'native-manifest.txt').write_text('5\n'+'\n'.join(rows)+'\n')
print('Five public finite fixtures mapped to exact compiled sources and actual resources')
