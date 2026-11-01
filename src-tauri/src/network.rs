//! 인터넷 연결 확인. 2초마다 TCP 접속을 시도하고 성공하면 끝낸다.

use crate::model::now_f;
use crate::platform::NET_PROBE;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

fn probe() -> bool {
    let Ok(addrs) = NET_PROBE.to_socket_addrs() else { return false };
    addrs.into_iter().take(2).any(|a| TcpStream::connect_timeout(&a, Duration::from_secs(2)).is_ok())
}

/// 연결 시각(기준 시각 대비 초)을 `out`에 기록한다.
pub fn spawn(anchor: f64, out: Arc<Mutex<Option<f64>>>) {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(330);
        while Instant::now() < deadline {
            if probe() {
                if let Ok(mut g) = out.lock() {
                    *g = Some(crate::model::round1((now_f() - anchor).max(0.0)));
                }
                return;
            }
            thread::sleep(Duration::from_secs(2));
        }
    });
}
