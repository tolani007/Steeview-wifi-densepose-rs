//! UDP adapter — receive CSI frames from ESP32 nodes over UDP.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::net::UdpSocket;
use tracing::{error, info};
use wifi_densepose_core::{CsiFrame, CsiMetadata, AntennaConfig, FrequencyBand};

pub struct UdpAdapter {
    bind_addr:   String,
    connected:   Arc<AtomicBool>,
    frame_count: Arc<AtomicU64>,
}

impl UdpAdapter {
    pub fn new(bind_addr: impl Into<String>) -> Self {
        Self {
            bind_addr: bind_addr.into(),
            connected: Arc::new(AtomicBool::new(false)),
            frame_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn is_connected(&self) -> bool { self.connected.load(Ordering::Relaxed) }
    pub fn frame_count(&self)  -> u64  { self.frame_count.load(Ordering::Relaxed) }

    /// Run the UDP receive loop — sends parsed frames to the provided sender.
    pub async fn run(
        &self,
        tx: crate::CsiSender,
        n_subcarriers: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let socket = UdpSocket::bind(&self.bind_addr).await?;
        self.connected.store(true, Ordering::Relaxed);
        info!(addr = %self.bind_addr, "UDP adapter listening");

        let mut buf = vec![0u8; 65536];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((len, _peer)) => {
                    debug_assert!(len > 0);
                    // Parse minimal ESP32 CSI packet: [frame_id u32 LE][n_sc u8][amp f32×n_sc][phase f32×n_sc]
                    if let Some(frame) = parse_esp32_packet(&buf[..len], n_subcarriers) {
                        self.frame_count.fetch_add(1, Ordering::Relaxed);
                        let _ = tx.send(Arc::new(frame));
                    }
                }
                Err(e) => {
                    error!("UDP recv error: {e}");
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }
}

fn parse_esp32_packet(data: &[u8], n_sc: usize) -> Option<CsiFrame> {
    if data.len() < 5 { return None; }
    let frame_id = u32::from_le_bytes(data[..4].try_into().ok()?) as u64;
    let n_links  = data[4] as usize;
    let needed   = 5 + n_links * n_sc * 2 * 4; // amp + phase, f32 each
    if data.len() < needed { return None; }

    let mut offset = 5usize;
    let mut amplitude = Vec::with_capacity(n_links);
    let mut phase     = Vec::with_capacity(n_links);

    for _ in 0..n_links {
        let amp_row: Vec<f32> = (0..n_sc)
            .map(|i| f32::from_le_bytes(data[offset + i*4..offset + i*4 + 4].try_into().unwrap_or([0;4])))
            .collect();
        offset += n_sc * 4;
        let ph_row: Vec<f32> = (0..n_sc)
            .map(|i| f32::from_le_bytes(data[offset + i*4..offset + i*4 + 4].try_into().unwrap_or([0;4])))
            .collect();
        offset += n_sc * 4;
        amplitude.push(amp_row);
        phase.push(ph_row);
    }

    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);

    Some(CsiFrame {
        metadata: CsiMetadata {
            timestamp: ts, frame_id,
            device_id: "esp32-udp".into(),
            antenna: AntennaConfig { n_tx: 1, n_rx: n_links, n_subcarriers: n_sc },
            band: FrequencyBand::channel_1(),
            rssi_dbm: -65.0, noise_floor_dbm: -90.0,
        },
        amplitude, phase,
    })
}
