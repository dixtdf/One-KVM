import pathlib,subprocess,sys
src=pathlib.Path(sys.argv[1]);out=pathlib.Path(sys.argv[2]);out.mkdir(parents=True,exist_ok=True)
s=(src/"src/otg/service.rs").read_text()
a=s.index("    fn owns_otg(&self)");b=s.index("    pub fn is_available()",a)
method=s[a:b].replace('"/run/lock/one-kvm-otg.lock"','std::env::temp_dir().join(format!("one-kvm-ownership-test-{}.lock", std::process::id()))')
code="""#[derive(Debug)] enum AppError { Config(String) }
type Result<T> = std::result::Result<T,AppError>;
struct OtgService { ownership_lock: std::sync::Mutex<Option<std::fs::File>> }
impl OtgService {
fn new()->Self { Self {ownership_lock:std::sync::Mutex::new(None)} }
"""+method+"""}
#[test] fn ownership_is_exclusive_and_none_does_not_lock(){
 std::env::set_var("ONE_KVM_OTG_OWNERSHIP","none");
 let none=OtgService::new();assert!(!none.owns_otg().unwrap());assert!(none.ownership_lock.lock().unwrap().is_none());
 std::env::set_var("ONE_KVM_OTG_OWNERSHIP","owner");
 let owner=OtgService::new();assert!(owner.owns_otg().unwrap());assert!(owner.owns_otg().unwrap());
 let other=OtgService::new();assert!(other.owns_otg().is_err());drop(owner);assert!(other.owns_otg().unwrap());
 std::env::set_var("ONE_KVM_OTG_OWNERSHIP","invalid");assert!(OtgService::new().owns_otg().is_err());
}
"""
(out/"test.rs").write_text(code)
subprocess.run(["rustc","--edition=2021","--test",str(out/"test.rs"),"-o",str(out/"test")],check=True)
subprocess.run([str(out/"test"),"--test-threads=1"],check=True)
