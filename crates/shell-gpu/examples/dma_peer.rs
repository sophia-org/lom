//! Bounded joint control: one Lom producer and one native-EGL capture
//! receiver, outside any Sophia session. Fixture protocol v1 on a connected
//! SOCK_SEQPACKET socket at the path in argv[1], which the receiver binds:
//!
//! - FRAME, producer to receiver, 52 bytes little-endian with one SCM_RIGHTS fd:
//!   `b"SDMA"`, version 1, seq, width, height, offset, stride, format XR24,
//!   pattern, then u64 modifier 0 and u64 source_bytes.
//! - DONE, receiver to producer, 16 bytes: `b"SACK"`, version 1, seq, status
//!   (0 complete and pixels match, 1 complete but mismatch, 2 refused before
//!   use, 3 uncertain).
//!
//! Pattern 1 is left half red (255, 0, 0), right half blue (0, 0, 255). Pattern 2 is left
//! green (0, 255, 0), right white. Both are 256x64 and opaque, split at x = 128; XR24
//! bytes are B, G, R, X and X is undefined. Frame 2 is a new allocation
//! rendered only after DONE 1, so the receiver can show capture 1 survived a
//! later producer render. Storage is never reused, so no ownership reacquire
//! exists. Each frame is retained until its DONE. Each DONE wait is 10 s; on
//! timeout or failure, frames are held until process exit. Run like
//! `dma_smoke`, with the socket directory bound into the domain.
use imaging::Painter;
use imaging::kurbo::Rect;
use imaging::peniko::Color;
use imaging::record::Scene;
use rustix::net::{self, SendAncillaryBuffer, SendAncillaryMessage, SendFlags};
use sophia_shell_gpu::{DmaFrame, GpuGrant, GpuPreview};
use std::os::fd::AsFd;
use std::time::Duration;

const WIDTH: u32 = 256;
const HEIGHT: u32 = 64;
const XR24: u32 = 0x3432_5258;
const PATTERNS: [[Color; 2]; 2] = [
    [Color::from_rgb8(255, 0, 0), Color::from_rgb8(0, 0, 255)],
    [Color::from_rgb8(0, 255, 0), Color::from_rgb8(255, 255, 255)],
];

fn main() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dma_peer <socket path>")?;
    let grant = GpuGrant::from_environment(1)?;
    let mut gpu = GpuPreview::new(&grant)?;
    println!("{}", gpu.evidence(&grant)?.record("dma_peer")?);
    let socket = net::socket(net::AddressFamily::UNIX, net::SocketType::SEQPACKET, None)
        .map_err(|error| format!("socket: {error}"))?;
    let address = net::SocketAddrUnix::new(path.as_str()).map_err(|e| format!("path: {e}"))?;
    net::connect(&socket, &address).map_err(|error| format!("connect: {error}"))?;
    net::sockopt::set_socket_timeout(
        &socket,
        net::sockopt::Timeout::Recv,
        Some(Duration::from_secs(10)),
    )
    .map_err(|error| format!("timeout: {error}"))?;
    let mut held = Vec::new();
    for (seq, pattern) in (1u32..).zip(PATTERNS) {
        let frame = gpu.render_dma_buf(&mut scene(pattern), WIDTH, HEIGHT)?;
        send(&socket, seq, &frame)?;
        println!(
            "dma_peer_frame schema=1 seq={seq} width={WIDTH} height={HEIGHT} offset={} stride={} source_bytes={}",
            frame.offset(),
            frame.stride(),
            frame.source_bytes()
        );
        // Retained, unchanged, until its DONE; also past it, until both are done.
        held.push(frame);
        let status = done(&socket, seq)?;
        println!("dma_peer_done schema=1 seq={seq} status={status}");
        if status != 0 {
            return Err(format!("receiver reported status {status} for seq {seq}"));
        }
    }
    drop(held);
    println!("dma_peer status=ok frames=2");
    Ok(())
}

fn scene([left, right]: [Color; 2]) -> Scene {
    let mut scene = Scene::new();
    {
        let mut painter = Painter::new(&mut scene);
        let (half, height) = (f64::from(WIDTH / 2), f64::from(HEIGHT));
        painter.fill_rect(Rect::new(0.0, 0.0, half, height), left);
        painter.fill_rect(Rect::new(half, 0.0, 2.0 * half, height), right);
    }
    scene
}

fn send(socket: &impl AsFd, seq: u32, frame: &DmaFrame) -> Result<(), String> {
    let mut packet = Vec::with_capacity(52);
    packet.extend_from_slice(b"SDMA");
    for word in [
        1,
        seq,
        frame.width(),
        frame.height(),
        frame.offset(),
        frame.stride(),
        XR24,
        seq,
    ] {
        packet.extend_from_slice(&u32::to_le_bytes(word));
    }
    packet.extend_from_slice(&0u64.to_le_bytes());
    packet.extend_from_slice(&frame.source_bytes().to_le_bytes());
    let fd = frame.dup_fd()?;
    let mut space = [std::mem::MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(1))];
    let mut control = SendAncillaryBuffer::new(&mut space);
    let fds = [fd.as_fd()];
    if !control.push(SendAncillaryMessage::ScmRights(&fds)) {
        return Err("SCM_RIGHTS space".into());
    }
    let sent = net::sendmsg(
        socket,
        &[std::io::IoSlice::new(&packet)],
        &mut control,
        SendFlags::NOSIGNAL,
    )
    .map_err(|error| format!("send FRAME {seq}: {error}"))?;
    if sent != packet.len() {
        return Err(format!("short FRAME {seq}"));
    }
    Ok(())
}

fn done(socket: &impl AsFd, seq: u32) -> Result<u32, String> {
    let mut reply = [0u8; 17];
    let (_, len) = net::recv(socket, &mut reply, net::RecvFlags::empty())
        .map_err(|error| format!("DONE {seq}: {error}"))?;
    let word = |at: usize| u32::from_le_bytes(reply[at..at + 4].try_into().expect("4 bytes"));
    if len != 16 || &reply[..4] != b"SACK" || word(4) != 1 || word(8) != seq {
        return Err(format!("malformed DONE for seq {seq}"));
    }
    Ok(word(12))
}
