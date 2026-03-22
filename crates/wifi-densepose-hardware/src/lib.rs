//! Hardware adapters: Simulated CSI, UDP receiver, PCAP replay.

use std::sync::Arc;
use tokio::sync::broadcast;
use wifi_densepose_core::CsiFrame;

pub mod simulated;
pub mod udp;

pub use simulated::SimulatedAdapter;
pub use udp::UdpAdapter;

/// Hardware adapter trait — provides a stream of CSI frames.
pub trait HardwareAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn is_connected(&self) -> bool;
    fn frame_count(&self) -> u64;
}

/// Shared channel for CSI frames from any adapter.
pub type CsiSender   = broadcast::Sender<Arc<CsiFrame>>;
pub type CsiReceiver = broadcast::Receiver<Arc<CsiFrame>>;

/// Create a broadcast channel for CSI frames.
pub fn csi_channel(capacity: usize) -> (CsiSender, CsiReceiver) {
    broadcast::channel(capacity)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_channel_creation() {
        let (tx, mut rx) = csi_channel(16);
        assert_eq!(tx.receiver_count(), 1);
        drop(rx);
        assert_eq!(tx.receiver_count(), 0);
    }
}
