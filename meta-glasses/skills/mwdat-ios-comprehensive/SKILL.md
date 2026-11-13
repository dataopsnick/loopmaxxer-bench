---
name: mwdat-ios-comprehensive
description: Complete iOS developer reference for the Meta Wearables Device Access Toolkit (DAT) SDK (v1.0.1). Covers setup, session lifecycle, taking pictures (in-stream & standalone HD), video streaming & recording, hardware inputs/software macros, voice conversation (ASR, Audio Streaming, TTS echo cancellation), Voice Invocations ("Hey Meta"), Display/IVR menus, VLM/tool-calling integration architectures, and MockDeviceKit testing.
---

# Meta Wearables Device Access Toolkit (DAT) — iOS Master Skill

## 1. SDK Architecture & Modules

The DAT SDK (`https://github.com/facebook/meta-wearables-dat-ios`, v1.0.1) is distributed via Swift Package Manager and organized into the following modules:

| Module | Purpose | Production Status |
|---|---|---|
| `MWDATCore` | `Wearables.shared`, registration, permissions, device discovery, `DeviceSelector`, `DeviceSession`, `VoiceInvocationsStream`, `ListenerTokenBag` | Public Preview |
| `MWDATCamera` | `Camera`, `Stream` (video/audio/in-stream photo), `Photo` (standalone high-res capture) | Stream: Public Preview<br>Audio & Standalone Photo: Experimental* |
| `MWDATDisplay` | `Display`, declarative HUD UI (`FlexBox`, `Text`, `Button`, `ButtonGroup`, `Image`, `Icon`, `VideoPlayer`) | Public Preview |
| `MWDATInputs` | `Inputs`, semantic hardware events (`nav`, `select`, `back`, `button`, `capture`, `drag`) | Experimental* |
| `MWDATMotion` | `Motion`, 6-DOF IMU & magnetometer streaming (`accelerometer`, `gyroscope`, `magnetometer`, `orientation`) | Experimental* |
| `MWDATSpeech` | `Speech`, on-glasses automatic speech recognition (partial & final transcripts) | Experimental* |
| `MWDATMockDevice` | `MockDeviceKit` for hardware-free simulation, pose/media injection, and local Chrome/in-app display preview | Dev / Testing |
| `MWDATMockDeviceTestClient` | `MockDeviceTestClient` for driving `MockDeviceKit` over HTTP from XCUITest processes | UI Testing |

*\*Experimental capabilities require Wearables Developer Center (WDC) capability approval and are available for development and beta release channels, not yet production channels.*

---

## 2. Project Setup, Info.plist & Registration

### 2.1 Required `Info.plist` Configuration
The SDK transports data across BLE, Bluetooth Classic (`ExternalAccessory`), and Wi-Fi (Bonjour). It validates `Info.plist` before opening links:

```xml
<!-- 1. URL Scheme for Meta AI App OAuth & Voice Launch Callbacks -->
<key>CFBundleURLTypes</key>
<array>
  <dict>
    <key>CFBundleTypeRole</key>
    <string>Editor</string>
    <key>CFBundleURLName</key>
    <string>$(PRODUCT_BUNDLE_IDENTIFIER)</string>
    <key>CFBundleURLSchemes</key>
    <array>
      <string>myexampleapp</string>
    </array>
  </dict>
</array>

<!-- 2. DAT SDK Configuration -->
<key>MWDAT</key>
<dict>
  <key>AppLinkURLScheme</key>
  <string>myexampleapp://</string>
  <!-- Leave empty, "0", or unexpanded $(META_APP_ID) in Developer Mode to skip attestation -->
  <key>MetaAppID</key>
  <string>$(META_APP_ID)</string>
  <key>ClientToken</key>
  <string>$(CLIENT_TOKEN)</string>
  <key>TeamID</key>
  <string>$(DEVELOPMENT_TEAM)</string>
  <key>Analytics</key>
  <dict>
    <key>OptOut</key>
    <true/>
  </dict>
  <key>CrashReporting</key>
  <dict>
    <key>OptOut</key>
    <true/>
  </dict>
</dict>

<!-- 3. Background Modes & Hardware Transports -->
<key>UIBackgroundModes</key>
<array>
  <string>processing</string>
  <string>bluetooth-central</string>
  <string>bluetooth-peripheral</string>
  <string>external-accessory</string>
  <string>audio</string> <!-- Required if recording audio or keeping pocket sessions alive -->
</array>
<key>UISupportedExternalAccessoryProtocols</key>
<array>
  <string>com.meta.ar.wearable</string>
</array>
<key>NSBluetoothAlwaysUsageDescription</key>
<string>Needed to connect to Meta AI Glasses.</string>
<key>NSLocalNetworkUsageDescription</key>
<string>This lets your phone find and connect to your glasses over Wi-Fi.</string>
<key>NSBonjourServices</key>
<array>
  <string>_bonjour._tcp</string>
</array>
