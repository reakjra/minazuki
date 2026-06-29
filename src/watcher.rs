use anyhow::{Context, Result};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

// oh my days
const NETLINK_CONNECTOR: libc::c_int = 11;
const NLMSG_NOOP: u16 = 1;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const CN_IDX_PROC: u32 = 1;
const CN_VAL_PROC: u32 = 1;
const PROC_CN_MCAST_LISTEN: u32 = 1;
const PROC_EVENT_EXEC: u32 = 0x0000_0002;
const PROC_EVENT_EXIT: u32 = 0x8000_0000;

const CN_MSG_HEADER_LEN: usize = 20;
const PROC_EVENT_HEADER_LEN: usize = 16;
const RECV_TIMEOUT_SECS: libc::time_t = 5;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Exec { pid: u32 },
    Exit { pid: u32 },
}

pub struct Watcher {
    fd: OwnedFd,
}

impl Watcher {
    pub fn new() -> Result<Self> {
        let raw = unsafe { libc::socket(libc::AF_NETLINK, libc::SOCK_DGRAM, NETLINK_CONNECTOR) };
        if raw < 0 {
            return Err(io::Error::last_os_error()).context("open netlink connector socket");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let pid = unsafe { libc::getpid() } as u32;

        let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        addr.nl_family = libc::AF_NETLINK as u16;
        addr.nl_pid = pid;
        addr.nl_groups = CN_IDX_PROC;
        let rc = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                &addr as *const _ as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(io::Error::last_os_error())
                .context("bind proc connector group (needs root)");
        }

        let msg = subscribe_msg(pid);
        let sent = unsafe { libc::send(fd.as_raw_fd(), msg.as_ptr() as *const _, msg.len(), 0) };
        if sent < 0 {
            return Err(io::Error::last_os_error()).context("subscribe to proc events");
        }

        let timeout = libc::timeval {
            tv_sec: RECV_TIMEOUT_SECS,
            tv_usec: 0,
        };
        let _ = unsafe {
            libc::setsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &timeout as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };

        Ok(Self { fd })
    }

    pub fn recv(&self) -> Result<Vec<Event>> {
        let mut buf = [0u8; 8192];
        let n = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buf.as_mut_ptr() as *mut _,
                buf.len(),
                0,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if matches!(
                err.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) {
                return Ok(Vec::new());
            }
            return Err(err).context("recv proc event");
        }
        Ok(parse_events(&buf[..n as usize]))
    }
}

fn subscribe_msg(pid: u32) -> Vec<u8> {
    let total: u32 = 40; // reminder that if adding and not updating this its gonna hurt
    let mut buf = Vec::with_capacity(total as usize);
    buf.extend_from_slice(&total.to_ne_bytes());
    buf.extend_from_slice(&NLMSG_DONE.to_ne_bytes());
    buf.extend_from_slice(&0u16.to_ne_bytes());
    buf.extend_from_slice(&0u32.to_ne_bytes());
    buf.extend_from_slice(&pid.to_ne_bytes());
    buf.extend_from_slice(&CN_IDX_PROC.to_ne_bytes());
    buf.extend_from_slice(&CN_VAL_PROC.to_ne_bytes());
    buf.extend_from_slice(&0u32.to_ne_bytes());
    buf.extend_from_slice(&0u32.to_ne_bytes());
    buf.extend_from_slice(&4u16.to_ne_bytes());
    buf.extend_from_slice(&0u16.to_ne_bytes());
    buf.extend_from_slice(&PROC_CN_MCAST_LISTEN.to_ne_bytes());
    buf
}

fn parse_events(buf: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut off = 0;
    while off + 16 <= buf.len() {
        let len = u32::from_ne_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        let ntype = u16::from_ne_bytes(buf[off + 4..off + 6].try_into().unwrap());
        if len < 16 || off + len > buf.len() {
            break;
        }
        if ntype != NLMSG_NOOP
            && ntype != NLMSG_ERROR
            && let Some(event) = parse_proc_event(&buf[off + 16..off + len])
        {
            events.push(event);
        }
        off += (len + 3) & !3;
    }
    events
}

fn parse_proc_event(payload: &[u8]) -> Option<Event> {
    let pe = payload.get(CN_MSG_HEADER_LEN..)?;
    let what = u32_at(pe, 0)?;
    let process_pid = u32_at(pe, PROC_EVENT_HEADER_LEN)?;
    let process_tgid = u32_at(pe, PROC_EVENT_HEADER_LEN + 4)?;
    match what {
        PROC_EVENT_EXEC => Some(Event::Exec { pid: process_tgid }),
        PROC_EVENT_EXIT if process_pid == process_tgid => Some(Event::Exit { pid: process_tgid }),
        _ => None,
    }
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    b.get(off..off + 4)
        .map(|s| u32::from_ne_bytes(s.try_into().unwrap()))
}
