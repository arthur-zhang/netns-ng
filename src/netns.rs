use std::fmt::{Display, Formatter};
use std::fs::{DirBuilder, File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use nix::sched::CloneFlags;

pub const BIND_MOUNT_PATH: &str = "/run/netns";

#[derive(Debug)]
pub struct Netns {
    f: File,
    path: Option<PathBuf>,
}

impl Netns {
    pub fn new() -> anyhow::Result<Self> {
        nix::sched::unshare(CloneFlags::CLONE_NEWNET)?;
        Self::get()
    }

    pub fn new_named(name: &str) -> anyhow::Result<Self> {
        let bind_mount_path: &Path = BIND_MOUNT_PATH.as_ref();
        if !bind_mount_path.exists() {
            DirBuilder::new().mode(0o755).recursive(true).create(bind_mount_path)?;
        }

        let named_path = bind_mount_path.join(name);

        let _ = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o444)
            .open(&named_path)?;

        let new_ns = Self::new()?;
        let ns_path = format!("/proc/{}/task/{}/ns/net", std::process::id(), nix::unistd::gettid());
        nix::mount::mount(
            Some(Path::new(&ns_path)),
            Path::new(&named_path),
            None::<&str>,
            nix::mount::MsFlags::MS_BIND,
            None::<&str>,
        )?;
        return Ok(new_ns);
    }
    pub fn delete_named(name: &str) -> anyhow::Result<()> {
        let path: &Path = BIND_MOUNT_PATH.as_ref();
        let named_path = path.join(name);
        if !named_path.exists() {
            return Ok(());
        }
        nix::mount::umount2(&named_path, nix::mount::MntFlags::MNT_DETACH)?;
        std::fs::remove_file(named_path)?;
        Ok(())
    }
    pub fn get_from_path(path: &Path) -> anyhow::Result<Option<Self>> {
        let file = OpenOptions::new().read(true).open(&path).ok();
        match file {
            None => Ok(None),
            Some(file) => Ok(Some(Self { f: file, path: Some(path.to_path_buf()) })),
        }
    }
    pub fn get_from_name(name: &str) -> anyhow::Result<Option<Self>> {
        let path: &Path = BIND_MOUNT_PATH.as_ref();
        let named_path = path.join(name);
        Self::get_from_path(&named_path)
    }

    pub fn get() -> anyhow::Result<Self> {
        let ns_path = format!("/proc/{}/task/{}/ns/net", std::process::id(), nix::unistd::gettid());
        let file = OpenOptions::new().read(true).open(Path::new(&ns_path))?;
        Ok(Self { f: file, path: None })
    }
    pub fn set(&self) -> anyhow::Result<()> {
        Ok(nix::sched::setns(self.f.as_fd(), CloneFlags::CLONE_NEWNET)?)
    }
    pub fn unique_id(&self) -> String {
        match self.f.metadata() {
            Err(_) => {
                "NS(unknown)".into()
            }
            Ok(metadata) => {
                format!("NS({}:{})", metadata.dev(), metadata.ino())
            }
        }
    }
    pub fn fd(&self) -> RawFd {
        self.f.as_raw_fd()
    }
    pub fn path(&self) -> Option<PathBuf> {
        self.path.clone()
    }
}
#[macro_export]
macro_rules! exec_netns {
    ($cur_ns:expr, $target_ns:expr, $result:ident, $exec:expr) => {
        $target_ns.set()?;
        let $result = $exec();
        $cur_ns.set()?;
    };
}

impl PartialEq<Self> for Netns {
    fn eq(&self, other: &Self) -> bool {
        if std::ptr::eq(self, other) {
            return true;
        }
        let self_meta = self.f.metadata();
        let other_meta = other.f.metadata();
        if self_meta.is_err() || other_meta.is_err() {
            return false;
        }
        let self_meta = self_meta.unwrap();
        let other_meta = other_meta.unwrap();
        return self_meta.dev() == other_meta.dev() && self_meta.ino() == other_meta.ino();
    }
}

impl Display for Netns {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.f.metadata() {
            Err(_) => {
                write!(f, "NS({}: unknown)", self.f.as_raw_fd())
            }
            Ok(metadata) => {
                write!(f, "NS({}: {}, {})", self.f.as_raw_fd(), metadata.dev(), metadata.ino())
            }
        }
    }
}

impl Eq for Netns {}


#[cfg(test)]
mod tests {
    use anyhow::bail;

    use super::*;

    #[test]
    fn test_set() {
        let last_ns = Netns::get().unwrap();
        println!("cur_ns: {}", last_ns.unique_id());
        let new_ns = Netns::new_named("hello").unwrap();
        println!("set ns to hello");
        new_ns.set().unwrap();
        let cur_ns = Netns::get().unwrap();
        println!("cur_ns: {}", cur_ns.unique_id());
        println!("set ns to last");
        last_ns.set().unwrap();
        println!("cur_ns: {}", last_ns.unique_id());
    }

    #[test]
    fn test_get_new_set_delete() {
        let origins = Netns::get();
        assert!(origins.is_ok());
        let origin_netns = origins.unwrap();
        let netns = Netns::new();
        assert!(netns.is_ok());
        let netns = netns.unwrap();
        assert_ne!(netns, origin_netns);

        let res = origin_netns.set();
        assert!(res.is_ok());

        let ns = Netns::get();
        assert!(ns.is_ok());
        let ns = ns.unwrap();
        assert_eq!(ns, origin_netns);
    }

    #[test]
    fn test_named() {
        let res = Netns::delete_named("test");
        assert!(res.is_ok());
        let netns = Netns::new_named("test");
        assert!(netns.is_ok());
        let netns = netns.unwrap();
        let netns2 = Netns::get_from_name("test");
        assert!(netns2.is_ok());
        let netns2 = netns2.unwrap();
        assert!(netns2.is_some());
        let netns2 = netns2.unwrap();
        println!("netns: {}, netns2: {}", netns.unique_id(), netns2.unique_id());
        assert_eq!(netns, netns2);
    }

    #[test]
    fn test_exec_ns() -> anyhow::Result<()> {
        let ns2 = Netns::get().unwrap();
        println!("ns2: {}", ns2.unique_id());
        let _ = Netns::delete_named("test1");
        let ns1 = Netns::new_named("test1").unwrap();
        println!("ns1: {}", ns1.unique_id());
        exec_netns!(ns2, ns1, result, || -> anyhow::Result<()>{
            let fn_ns = Netns::get().unwrap();
            println!("fn_ns: {}", fn_ns.unique_id());
            bar()?;
            foo()
        });
        let a = result;
        println!("result is : {:?}", a);
        let ns3 = Netns::get().unwrap();
        println!("ns3: {}", ns3.unique_id());
        Ok(())
    }

    fn foo() -> anyhow::Result<()> {
        bail!("me..........")
    }

    fn bar() -> anyhow::Result<()> {
        bail!("me..........")
    }
}