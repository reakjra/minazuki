use anyhow::{Context, Result};
use std::ffi::{CString, OsString};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const MASK: u32 = libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO;
const EVENT_SIZE: usize = std::mem::size_of::<libc::inotify_event>();

#[repr(align(8))]
struct Buffer([u8; 4096]);

pub struct Watch {
    fd: OwnedFd,
    file: OsString,
}

impl Watch {
    pub fn new(path: &Path) -> Result<Self> {
        let dir = path.parent().context("no parent directory")?;
        let file = path.file_name().context("no file name")?.to_os_string();

        let raw = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        if raw < 0 {
            return Err(io::Error::last_os_error()).context("inotify_init1");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };

        // editors rename a temp file over the target, orphaning a watch on the file
        // itself, so the directory is the only thing that stays put
        let target = CString::new(dir.as_os_str().as_bytes())?;
        if unsafe { libc::inotify_add_watch(fd.as_raw_fd(), target.as_ptr(), MASK) } < 0 {
            return Err(io::Error::last_os_error())
                .with_context(|| format!("watch {}", dir.display()));
        }
        Ok(Self { fd, file })
    }

    pub fn touched(&self) -> bool {
        let mut buf = Buffer([0; 4096]);
        let mut hit = false;
        loop {
            let read =
                unsafe { libc::read(self.fd.as_raw_fd(), buf.0.as_mut_ptr().cast(), buf.0.len()) };
            let Ok(read) = usize::try_from(read) else {
                return hit;
            };
            if read == 0 {
                return hit;
            }
            let mut offset = 0;
            while offset + EVENT_SIZE <= read {
                let event = unsafe { &*(buf.0.as_ptr().add(offset) as *const libc::inotify_event) };
                let start = offset + EVENT_SIZE;
                let end = start + event.len as usize;
                if end > read {
                    break;
                }
                let name = buf.0[start..end]
                    .split(|&b| b == 0)
                    .next()
                    .unwrap_or_default();
                hit |= name == self.file.as_os_str().as_bytes();
                offset = end;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn follows_writes_renames_and_ignores_neighbours() {
        let dir = std::env::temp_dir().join(format!("minazuki-inotify-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("config.toml");
        let watch = Watch::new(&target).unwrap();
        assert!(!watch.touched(), "nothing happened yet");

        fs::write(&target, "games = []").unwrap();
        assert!(watch.touched(), "a plain write");
        assert!(!watch.touched(), "events are drained");

        let temp = dir.join("config.toml.tmp");
        fs::write(&temp, "games = []").unwrap();
        fs::rename(&temp, &target).unwrap();
        assert!(watch.touched(), "the way editors actually save");

        fs::write(dir.join("other.toml"), "x").unwrap();
        assert!(!watch.touched(), "a different file in the same directory");

        fs::remove_dir_all(&dir).unwrap();
    }
}
