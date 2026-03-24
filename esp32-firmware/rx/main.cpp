/**
 * Steeview ESP32 RX Firmware
 * ─────────────────────────────────────────────────────────────────────────────
 * Role:
 *   1. Connect to the TX's SoftAP (SSID: SteeviewCSI)
 *   2. Enable custom CSI callback via esp_wifi_set_csi()
 *   3. For every CSI packet received: extract amplitude + phase, pack into a
 *      binary UDP datagram, and send to the Mac's IP on port 5500.
 *
 * UDP Packet Format (matches wifi-densepose-rs/crates/wifi-densepose-hardware/src/udp.rs):
 *
 *   [frame_id: u32 LE]        4 bytes
 *   [n_links:  u8]            1 byte  (= 1 for single RX antenna)
 *   [amp_f32 × N_SUBCARRIERS] N*4 bytes
 *   [phase_f32 × N_SUBCARRIERS] N*4 bytes
 *   Total: 5 + 2*N*4 bytes
 *
 * For N_SUBCARRIERS=56: 5 + 448 = 453 bytes per packet
 */

#include <Arduino.h>
#include <WiFi.h>
#include <WiFiUdp.h>
#include "esp_wifi.h"
#include "esp_private/wifi.h"
#include <math.h>

#ifndef WIFI_SSID
  #define WIFI_SSID "SteeviewCSI"
#endif
#ifndef WIFI_PASS
  #define WIFI_PASS "steeview2026"
#endif
#ifndef MAC_IP
  #define MAC_IP "192.168.4.2"   // fallback — override in platformio.ini
#endif
#ifndef UDP_PORT
  #define UDP_PORT 5500
#endif
#ifndef N_SUBCARRIERS
  #define N_SUBCARRIERS 56
#endif

#define WIFI_CHANNEL 6

// ─── Globals ──────────────────────────────────────────────────────────────────
static WiFiUDP udp;
static uint32_t frame_counter = 0;
static volatile bool csi_ready = false;

// CSI callback buffer (filled in ISR context, read in loop)
static float amp_buf[N_SUBCARRIERS];
static float phase_buf[N_SUBCARRIERS];
static portMUX_TYPE csi_mux = portMUX_INITIALIZER_UNLOCKED;

// ─── CSI Callback ─────────────────────────────────────────────────────────────
/**
 * Called by the ESP32 WiFi driver for every received management/data frame.
 * We parse the raw CSI buffer into amplitude + phase and flag the main loop.
 *
 * ⚠️  This runs at interrupt level — keep it short, no heap allocations.
 */
static void IRAM_ATTR csi_callback(void *ctx, wifi_csi_info_t *info) {
  if (!info || !info->buf) return;

  const int8_t *raw = info->buf;
  int len = info->len;          // len = 2 × n_subcarriers (I/Q interleaved)
  int n_sc = len / 2;
  if (n_sc > N_SUBCARRIERS) n_sc = N_SUBCARRIERS;

  portENTER_CRITICAL_ISR(&csi_mux);
  for (int i = 0; i < n_sc; i++) {
    float I = (float)raw[2*i];
    float Q = (float)raw[2*i + 1];
    amp_buf[i]   = sqrtf(I*I + Q*Q);
    phase_buf[i] = atan2f(Q, I);
  }
  // Zero-pad if driver returns fewer subcarriers than expected
  for (int i = n_sc; i < N_SUBCARRIERS; i++) {
    amp_buf[i] = 0.0f;
    phase_buf[i] = 0.0f;
  }
  csi_ready = true;
  portEXIT_CRITICAL_ISR(&csi_mux);
}

// ─── Setup ────────────────────────────────────────────────────────────────────
void setup() {
  Serial.begin(115200);
  delay(500);

  Serial.println("=== Steeview RX Firmware ===");
  Serial.printf("Target Mac IP: %s:%d\n", MAC_IP, UDP_PORT);
  Serial.printf("Expected subcarriers: %d\n", N_SUBCARRIERS);

  // Connect to TX SoftAP
  WiFi.mode(WIFI_STA);
  WiFi.begin(WIFI_SSID, WIFI_PASS);
  Serial.print("Connecting to TX AP");
  uint32_t t0 = millis();
  while (WiFi.status() != WL_CONNECTED) {
    delay(250);
    Serial.print(".");
    if (millis() - t0 > 15000) {
      Serial.println("\n[ERROR] Could not connect to TX AP. Check TX is powered and in range.");
      // Retry after 5s
      delay(5000);
      ESP.restart();
    }
  }
  Serial.printf("\nConnected! RX IP: %s\n", WiFi.localIP().toString().c_str());

  // ── Enable CSI ────────────────────────────────────────────────────────────
  esp_wifi_set_promiscuous(false); // CSI works in normal (non-promiscuous) mode

  wifi_csi_config_t csi_cfg = {};
  csi_cfg.lltf_en           = true;   // Legacy Long Training Field — most reliable
  csi_cfg.htltf_en          = false;
  csi_cfg.stbc_htltf2_en    = false;
  csi_cfg.ltf_merge_en      = true;
  csi_cfg.channel_filter_en = false;  // disable HW filter to get raw I/Q
  csi_cfg.manu_scale        = false;

  ESP_ERROR_CHECK(esp_wifi_set_csi_config(&csi_cfg));
  ESP_ERROR_CHECK(esp_wifi_set_csi_rx_cb(csi_callback, NULL));
  ESP_ERROR_CHECK(esp_wifi_set_csi(true));

  udp.begin(4444); // local port (arbitrary — we only SEND)

  Serial.println("CSI extraction active — streaming UDP to Mac");
}

// ─── UDP packet send ─────────────────────────────────────────────────────────
static void send_csi_packet(uint32_t fid, const float *amp, const float *phase) {
  // Packet size: 4 (frame_id) + 1 (n_links=1) + N_SUBCARRIERS*4 (amp) + N_SUBCARRIERS*4 (phase)
  const size_t PKT_SIZE = 5 + N_SUBCARRIERS * 4 * 2;
  static uint8_t pkt[5 + N_SUBCARRIERS * 4 * 2];

  // frame_id (u32 LE)
  pkt[0] = (fid >>  0) & 0xFF;
  pkt[1] = (fid >>  8) & 0xFF;
  pkt[2] = (fid >> 16) & 0xFF;
  pkt[3] = (fid >> 24) & 0xFF;
  // n_links = 1
  pkt[4] = 1;

  // amplitude f32 × N_SUBCARRIERS
  memcpy(pkt + 5, amp, N_SUBCARRIERS * 4);
  // phase f32 × N_SUBCARRIERS
  memcpy(pkt + 5 + N_SUBCARRIERS * 4, phase, N_SUBCARRIERS * 4);

  udp.beginPacket(MAC_IP, UDP_PORT);
  udp.write(pkt, PKT_SIZE);
  udp.endPacket();
}

// ─── Main loop ────────────────────────────────────────────────────────────────
void loop() {
  if (csi_ready) {
    // Copy out of ISR buffer quickly
    float amp[N_SUBCARRIERS], phase[N_SUBCARRIERS];
    portENTER_CRITICAL(&csi_mux);
    memcpy(amp,   amp_buf,   sizeof(amp));
    memcpy(phase, phase_buf, sizeof(phase));
    csi_ready = false;
    portEXIT_CRITICAL(&csi_mux);

    send_csi_packet(frame_counter++, amp, phase);

    // Debug print every 100 frames (~1s at 100 Hz)
    if (frame_counter % 100 == 0) {
      Serial.printf("[RX] frame=%lu  amp[0]=%.2f  phase[0]=%.3f  wifi_rssi=%d dBm\n",
                    (unsigned long)frame_counter,
                    amp[0], phase[0],
                    WiFi.RSSI());
    }
  }

  // Reconnect if WiFi drops
  if (WiFi.status() != WL_CONNECTED) {
    Serial.println("[RX] WiFi lost — reconnecting...");
    WiFi.reconnect();
    delay(2000);
  }
}
