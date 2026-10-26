import pathlib,json,subprocess,sys,hashlib
root=pathlib.Path(__file__).resolve().parents[1]
src=pathlib.Path(sys.argv[1]).resolve()
pins=json.loads((root/"pins.json").read_text())
def change(path,old,new):
 p=src/path;s=p.read_text()
 assert s.count(old)==1,(path,old[:90],s.count(old))
 p.write_text(s.replace(old,new),newline="\n")
head=subprocess.check_output(["git","rev-parse","HEAD"],cwd=src,text=True).strip()
assert head==pins["upstream"],head
patch=root/"patches/0001-custom-rustdesk-id-key.patch"
for item in sorted((root/"patches").glob("*.patch")):
 subprocess.run(["git","apply","--check",str(item)],cwd=src,check=True)
 subprocess.run(["git","apply",str(item)],cwd=src,check=True)
change("Cargo.toml",'version = "0.2.8"','version = "'+pins["version"]+'"')
change("Cargo.lock",'name = "one-kvm"\nversion = "0.2.8"','name = "one-kvm"\nversion = "'+pins["version"]+'"')
p=src/"build/cross/Dockerfile.arm64";s=p.read_text()
s=s.replace("ARG DEBIAN_IMAGE=debian:12","ARG DEBIAN_IMAGE="+pins["debian"])
s=s.replace("--default-toolchain stable","--default-toolchain "+pins["rust"])
prefix=chr(36)+"{github_prefix}"
old='git clone --depth 1 "'+prefix+'https://github.com/mirror/x264.git" /tmp/x264'
new='git init /tmp/x264 && cd /tmp/x264 && git remote add origin "'+prefix+'https://github.com/mirror/x264.git" && git fetch --depth 1 origin '+pins["x264"]+' && git checkout --detach FETCH_HEAD'
assert old in s;s=s.replace(old,new)
for folder,repo,branch,key in [('rkmpp','mpp','RKMPP_BRANCH','mpp'),('rkrga','rk-mirrors','RKRGA_BRANCH','rga')]:
 old='git clone --depth 1 --branch '+chr(36)+'{'+branch+'} "'+prefix+'https://github.com/nyanmisaka/'+repo+'.git" '+folder
 new='git init '+folder+' && git -C '+folder+' remote add origin "'+prefix+'https://github.com/nyanmisaka/'+repo+'.git" && git -C '+folder+' fetch --depth 1 origin '+pins[key]+' && git -C '+folder+' checkout --detach FETCH_HEAD'
 assert old in s;s=s.replace(old,new)
p.write_text(s,newline="\n")
p=src/"build/build-images.sh";s=p.read_text().replace("cross build --release --target","cross build --locked --release --target");p.write_text(s,newline="\n")
p=src/"build/package-deb.sh";s=p.read_text();anchor="    # Build deb directly on host"
s=s.replace(anchor,'    mkdir -p "$PKG_DIR/usr/share/doc/one-kvm"\n    cp "$PROJECT_DIR/LICENSE" "$PKG_DIR/usr/share/doc/one-kvm/copyright"\n    cp -r "$PROJECT_DIR/licenses" "$PKG_DIR/usr/share/doc/one-kvm/licenses"\n    cp "'+str(root/'README.md')+'" "$PKG_DIR/usr/share/doc/one-kvm/CUSTOM-BUILD.md"\n'+anchor)
p.write_text(s,newline="\n")
out=root/"out";out.mkdir(exist_ok=True)
(out/"provenance.json").write_text(json.dumps({"upstream_commit":head,"pins":pins,"patch_sha256":hashlib.sha256(patch.read_bytes()).hexdigest()},indent=2))
(out/"source-changes.patch").write_bytes(subprocess.check_output(["git","diff","--binary"],cwd=src))
print("Pinned source and custom patch prepared")
