"""Package experimental manager without enabling services or changing live KVM."""
import hashlib,json,pathlib,shutil,subprocess,tarfile
root=pathlib.Path(__file__).resolve().parents[1]
out=root/'platform-out';out.mkdir(exist_ok=True)
stage=out/'package';assert not stage.exists(), 'Use a clean build workspace'
binary=stage/'usr/lib/one-kvm-platform/one-kvm-platform-service';binary.parent.mkdir(parents=True)
shutil.copy2(root/'platform-service/target/release/one-kvm-platform-service',binary)
binary.chmod(0o755)
control=stage/'DEBIAN';control.mkdir()
(control/'control').write_text('Package: one-kvm-platform-experimental\nVersion: 0.1.0~dev1\nArchitecture: arm64\nMaintainer: One-KVM custom build\nDepends: libc6 (>= 2.39)\nDescription: Experimental multi-channel manager; explicit initialization required\n')
docs=stage/'usr/share/doc/one-kvm-platform-experimental';docs.mkdir(parents=True)
for name in ['architecture.md','upstream-maintenance.md','development-log.md']:shutil.copy2(root/'docs'/name,docs/name)
shutil.copy2(root/'LICENSE',docs/'copyright')
deb=out/'one-kvm-platform-experimental_0.1.0~dev1_arm64.deb'
subprocess.run(['dpkg-deb','--root-owner-group','--build',str(stage),str(deb)],check=True)
with tarfile.open(out/'corresponding-platform-source.tar.gz','w:gz') as tar:
 for name in ['platform-core','platform-service','scripts','docs','LICENSE','.github/workflows/platform-release.yml']:
  tar.add(root/name,arcname=name,filter=lambda info:None if any(p in ['target','__pycache__'] for p in pathlib.PurePosixPath(info.name).parts) else info)
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
(out/'provenance.json').write_text(json.dumps({'commit':commit,'architecture':'arm64','rust':'1.99.1','experimental':True,'auto_start':False,'runtime_complete':False},indent=2))
files=[p for p in out.iterdir() if p.is_file()]
(out/'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+p.name+'\n' for p in sorted(files)))
shutil.rmtree(stage)
