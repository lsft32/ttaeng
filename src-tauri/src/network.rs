//! 인터넷 연결 확인. 2초마다 TCP 접속을 시도하고 성공하면 끝낸다.

use crate::model::now_f;
use crate::platform::NET_PROBE;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetHit {
    /// 연결을 확인한 시각(기준 시각 대비 초)
    pub t: f64,
    /// 첫 시도에서 바로 성공했는지. 그렇다면 측정 시작 전에 이미 연결돼 있었으므로 실제 연결 시각은 알 수 없다.
    pub first_try: bool,
}

/// 판정 엔진에 넘길 연결 시각. 측정 전부터 연결돼 있었다면 사용 가능 시점을 정하지 않도록 0으로 본다.
pub fn judge_time(hit: Option<NetHit>) -> Option<f64> {
    hit.map(|h| if h.first_try { 0.0 } else { h.t })
}

/// 기록에 남길 연결 시각. 알 수 없으면 None.
pub fn record_time(hit: Option<NetHit>) -> Option<f64> {
    hit.filter(|h| !h.first_try).map(|h| h.t)
}

fn probe() -> bool {
    let Ok(addrs) = NET_PROBE.to_socket_addrs() else { return false };
    addrs.into_iter().take(2).any(|a| TcpStream::connect_timeout(&a, Duration::from_secs(2)).is_ok())
}

pub fn spawn(anchor: f64, out: Arc<Mutex<Option<NetHit>>>) {
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(330);
        let mut first_try = true;
        while Instant::now() < deadline {
            if probe() {
                if let Ok(mut g) = out.lock() {
                    *g = Some(NetHit { t: crate::model::round1((now_f() - anchor).max(0.0)), first_try });
                }
                return;
            }
            first_try = false;
            thread::sleep(Duration::from_secs(2));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_before_measuring_is_unknown() {
        let pre = Some(NetHit { t: 257.1, first_try: true });
        assert_eq!(judge_time(pre), Some(0.0));
        assert_eq!(record_time(pre), None);
        let later = Some(NetHit { t: 24.0, first_try: false });
        assert_eq!(judge_time(later), Some(24.0));
        assert_eq!(record_time(later), Some(24.0));
        assert_eq!(judge_time(None), None);
    }
}
