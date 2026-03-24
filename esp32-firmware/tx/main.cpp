/**
 * Steeview ESP32 TX Firmware
 * ─────────────────────────────────────────────────────────────────────────────
 * Role: Continuously transmit WiFi NDPA (sounding) frames on a fixed channel
 *       so the RX unit can extract CSI from them.
 *
 * This is the simpler of the two firmwares — just keep sending beacon traffic.
 */ 

#include <Arduino.h>
#include <WiFi.h>

#ifndef WIFI_SSID
  #define WIFI_SSID "SteeviewCSI"
#endif
#ifndef WIFI_PASS
  #define WIFI_PASS "steeview2026"
#endif

// WiFi channel must match the RX firmware exactly
#define WIFI_CHANNEL 6

void setup() {
  Serial.begin(115200);
  delay(500);

  Serial.println("=== Steeview TX Firmware ===");
  Serial.printf("SSID: %s  Channel: %d\n", WIFI_SSID, WIFI_CHANNEL);

  // Start as a SoftAP so the RX can associate and receive our beacon frames
  WiFi.mode(WIFI_AP);
  WiFi.softAP(WIFI_SSID, WIFI_PASS, WIFI_CHANNEL, 0 /*hidden=false*/, 4 /*max_conn*/);

  IPAddress ip = WiFi.softAPIP();
  Serial.printf("TX AP started — IP: %s\n", ip.toString().c_str());
  Serial.println("TX active — sending beacon frames at ~100 Hz on channel 6");
}

void loop() {
  // ESP32 AP continuously sends beacons — nothing to do here.
  // Optionally blink LED on GPIO2 to indicate liveness.
  static uint32_t last = 0;
  if (millis() - last > 1000) {
    last = millis();
    Serial.printf("[TX] uptime: %lu s  station_count: %d\n",
                  millis() / 1000,
                  WiFi.softAPgetStationNum());
  }
  delay(10);
}
