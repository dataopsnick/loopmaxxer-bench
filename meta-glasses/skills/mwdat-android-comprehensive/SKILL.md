```markdown
---
name: mwdat-android-comprehensive
description: Complete Android developer reference for the Meta Wearables Device Access Toolkit (DAT) SDK (v1.0.1). Covers Gradle/Manifest setup, session lifecycle, taking pictures (in-stream & standalone HD), video streaming & passthrough MP4 recording, hardware inputs & IMU software macros, voice conversation (on-device ASR, PCM/HFP audio, TTS echo suppression, "Hey Meta" voice invocations), HUD Display & IVR menus, VLM/tool-calling integration architectures, and MockDeviceKit testing.
---

# Meta Wearables Device Access Toolkit (DAT) — Android Master Skill

## 1. SDK Architecture & Modules

The Meta Wearables Device Access Toolkit for Android (`com.meta.wearable`, v1.0.1) is published directly to **Maven Central** (no credentials or custom Maven repositories required).

| Artifact (`com.meta.wearable:*`) | Purpose | Production Status |
|---|---|---|
| `mwdat-core` | `Wearables`, registration, permissions, device discovery & state, `DeviceSelector`, `DeviceSession`, `VoiceInvocationsStream`, `DatResult` | Public Preview (`VoiceInvocations`: Experimental*) |
| `mwdat-camera` | `Camera`, `Stream` (video frames, synchronized PCM audio, in-stream photo capture), `Photo` (standalone high-res capture) | `Stream`: Public Preview<br>Audio & `Photo`: Experimental* |
| `mwdat-display` | `Display`, declarative HUD UI DSL (`flexBox`, `text`, `button`, `buttonGroup`, `image`, `icon`, `video`) | Public Preview |
| `mwdat-inputs` | `Inputs`, semantic hardware interactions (`Nav`, `Select`, `Back`, `Button`, `Capture`, `Drag`) | Experimental* |
| `mwdat-motion` | `Motion`, inertial sensor streaming (`MotionSample` with `Vector3` accel/gyro/mag and `Quaternion` orientation) | Experimental* |
| `mwdat-speech` | `Speech`, on-glasses automatic speech recognition (`TranscriptionResult` partial & final text) | Experimental* |
| `mwdat-mockdevice` | `MockDeviceKit` for hardware-free simulation, media/pose/speech injection, and in-app/Chrome display preview | Dev / Testing |

*\*Experimental capabilities require Wearables Developer Center (WDC) capability approval and can be used in development and beta release channels, not yet production release channels.*

---

## 2. Project Setup, Manifest & Registration

### 2.1 Gradle Dependencies (`libs.versions.toml` & `app/build.gradle.kts`)

```toml
# gradle/libs.versions.toml
[versions]
mwdat = "1.0.1"

[libraries]
mwdat-core = { group = "com.meta.wearable", name = "mwdat-core", version.ref = "mwdat" }
mwdat-camera = { group = "com.meta.wearable", name = "mwdat-camera", version.ref = "mwdat" }
mwdat-display = { group = "com.meta.wearable", name = "mwdat-display", version.ref = "mwdat" }
mwdat-inputs = { group = "com.meta.wearable", name = "mwdat-inputs", version.ref = "mwdat" }
mwdat-motion = { group = "com.meta.wearable", name = "mwdat-motion", version.ref = "mwdat" }
mwdat-speech = { group = "com.meta.wearable", name = "mwdat-speech", version.ref = "mwdat" }
mwdat-mockdevice = { group = "com.meta.wearable", name = "mwdat-mockdevice", version.ref = "mwdat" }
```

```kotlin
// app/build.gradle.kts
android {
    defaultConfig {
        minSdk = 31
        targetSdk = 36
        // Use "0" in Developer Mode (attestation is skipped).
        // Replace with Wearables Developer Center credentials for release channels.
        manifestPlaceholders["mwdat_application_id"] =
            localProperties.getProperty("mwdat_application_id", "0")
        manifestPlaceholders["mwdat_client_token"] =
            localProperties.getProperty("mwdat_client_token", "0")
    }
}

dependencies {
    implementation(libs.mwdat.core)
    implementation(libs.mwdat.camera)
    implementation(libs.mwdat.display)
    implementation(libs.mwdat.inputs)
    implementation(libs.mwdat.motion)
    implementation(libs.mwdat.speech)
    implementation(libs.mwdat.mockdevice)
}
```

### 2.2 Required `AndroidManifest.xml` Configuration

```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <!-- Required for DAT Bluetooth Classic / BLE link and attestation -->
    <uses-permission android:name="android.permission.BLUETOOTH" />
    <uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />
    <uses-permission android:name="android.permission.INTERNET" />

    <!-- Optional: Required if using HFP Bluetooth SCO mic, phone mic fallback, or MockDevice LIVE_DEVICE_ASR -->
    <uses-permission android:name="android.permission.RECORD_AUDIO" />
    <uses-permission android:name="android.permission.MODIFY_AUDIO_SETTINGS" />

    <!-- Optional: Required if using MockDeviceKit phone camera feed (setCameraFeed(CameraFacing)) -->
    <uses-permission android:name="android.permission.CAMERA" />

    <!-- Optional: Keep sessions/streams alive when phone is locked or backgrounded -->
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_MICROPHONE" />
    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-permission android:name="android.permission.WAKE_LOCK" />

    <application ...>
        <!-- Required: WDC Attestation Identifiers ("0" in Developer Mode) -->
        <meta-data
            android:name="com.meta.wearable.mwdat.APPLICATION_ID"
            android:value="${mwdat_application_id}" />
        <meta-data
            android:name="com.meta.wearable.mwdat.CLIENT_TOKEN"
            android:value="${mwdat_client_token}" />

        <!-- Optional: Opt out of SDK telemetry & SDK crash handler chaining -->
        <meta-data
            android:name="com.meta.wearable.mwdat.ANALYTICS_OPT_OUT"
            android:value="true" />
        <meta-data
            android:name="com.meta.wearable.mwdat.CRASH_REPORTING_OPT_OUT"
            android:value="true" />

        <!-- Use singleTask so Meta AI OAuth callbacks and Voice Invocations land in the existing Activity -->
        <activity
            android:name=".MainActivity"
            android:exported="true"
            android:launchMode="singleTask"
            android:screenOrientation="portrait">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>

            <!-- Callback URI scheme registered in Wearables Developer Center -->
            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="myexampleapp" />
            </intent-filter>
        </activity>

        <service
            android:name=".session.StreamingService"
            android:exported="false"
            android:foregroundServiceType="connectedDevice|microphone" />
    </application>
</manifest>
```

### 2.3 Initialization, `DatResult` Conventions & Registration

**Critical Ordering Rule on Android 12+ (API 31+):** Request `Manifest.permission.BLUETOOTH_CONNECT` **before** calling `Wearables.initialize(context)`. `Wearables.initialize` queries bonded Bluetooth devices; without `BLUETOOTH_CONNECT`, Android throws a `SecurityException` that the SDK catches and treats as "no glasses paired", leaving `Wearables.registrationState` stuck in `UNAVAILABLE` for the life of the process.

```kotlin
import com.meta.wearable.dat.core.Wearables
import com.meta.wearable.dat.core.types.Permission
import com.meta.wearable.dat.core.types.PermissionStatus
import com.meta.wearable.dat.core.types.RegistrationState

// 1. Initialize after BLUETOOTH_CONNECT is granted
Wearables.initialize(applicationContext)
    .onSuccess { /* Ready */ }
    .onFailure { error, cause ->
        // Always use the 2-parameter onFailure { error, _ -> } or .fold() to access typed DatError.description
        Log.e("DAT", "Init failed: ${error.description}", cause)
    }

// 2. Observe Registration State & Errors
lifecycleScope.launch {
    Wearables.registrationState.collect { state: RegistrationState ->
        // UNAVAILABLE, AVAILABLE, REGISTERING, REGISTERED, UNREGISTERING
    }
}
lifecycleScope.launch {
    Wearables.registrationErrorStream.collect { error ->
        Log.e("DAT", "Registration error: ${error.getLocalizedDescription(context)}")
    }
}

// 3. Trigger Registration / Unregistration (requires Activity)
fun register(activity: Activity) = Wearables.startRegistration(activity)
fun unregister(activity: Activity) = Wearables.startUnregistration(activity)
```

### 2.4 Requesting On-Glasses Permissions (`Permission.CAMERA`, `Permission.MICROPHONE`)
On-glasses permissions are managed by the Meta AI companion app, separate from Android OS permissions. Checking or requesting them requires an active link to the glasses (`NO_DEVICE_WITH_CONNECTION` is returned if glasses are folded/disconnected).

```kotlin
// Register the ActivityResultLauncher on your ComponentActivity
private val datPermissionLauncher =
    registerForActivityResult(Wearables.RequestPermissionContract()) { result ->
        result.onSuccess { status: PermissionStatus ->
            if (status == PermissionStatus.Granted) { /* Proceed */ }
        }.onFailure { error, _ ->
            Log.e("DAT", "Permission request failed: ${error.description}")
        }
    }

// Check before requesting (suspend function)
suspend fun ensureGlassesPermission(permission: Permission): Boolean {
    var granted = false
    Wearables.checkPermissionStatus(permission)
        .onSuccess { status -> granted = (status == PermissionStatus.Granted) }
        .onFailure { error, _ -> Log.w("DAT", "Cannot read permission: ${error.description}") }
    if (!granted) {
        datPermissionLauncher.launch(permission) // Redirects to Meta AI app
    }
    return granted
}
```

---

## 3. Device Discovery, Telemetry & Session Lifecycle

### 3.1 Observing Devices, Compatibility & Live Telemetry
`Wearables.devices` emits `Set<DeviceIdentifier>`. Per-device live state (`linkState`, `compatibility`, `batteryLevel`, `chargingState`, `donState`, `hingeState`, `thermalLevel`, `isDisplayCapable()`) is observed via `Wearables.devicesMetadata[id]`:

```kotlin
// Custom ranking for AutoDeviceSelector: prefer worn, connected glasses when multiple pairs exist
val wearablesSelector = AutoDeviceSelector(
    filter = { device -> device.compatibility == DeviceCompatibility.COMPATIBLE },
    deviceRanking = { devices ->
        devices.sortedWith(
            compareByDescending<Device> { it.linkState == LinkState.CONNECTED }
                .thenByDescending { it.donState == DonState.DONNED }
        )
    }
)

// Observe live device metadata and handle firmware / glasses app updates
Wearables.devices.collect { ids ->
    ids.forEach { id ->
        launch {
            Wearables.devicesMetadata[id]?.collect { device: Device ->
                if (device.compatibility == DeviceCompatibility.DEVICE_UPDATE_REQUIRED) {
                    Wearables.openFirmwareUpdate(activity)
                }
                val isWorn = device.donState == DonState.DONNED
                val battery = device.batteryLevel // 0..100
                val isHot = device.thermalLevel >= ThermalLevel.SEVERE
            }
        }
    }
}
```

### 3.2 `DeviceSession` Lifecycle
- `DeviceSession.start()` and `stop()` are synchronous fire-and-forget calls returning `Unit`.
- **Never attach capabilities (`addCamera`, `addDisplay`, `addInputs`, `addMotion`, `addSpeech`) immediately after `session.start()`**—doing so fails with `DeviceSessionError.SESSION_IDLE`. Always attach capabilities inside the `session.state` collector when `state == DeviceSessionState.STARTED`.
- `DeviceSessionState.STOPPED` is terminal. Do not reuse a stopped session; call `Wearables.createSession(...)` again.
- Unregistering the app (`Wearables.startUnregistration`) does **not** automatically stop a running `DeviceSession`; observe `Wearables.registrationState` and call `session.stop()` when it leaves `REGISTERED`.

```kotlin
Wearables.createSession(wearablesSelector).fold(
    onSuccess = { session ->
        // Subscribe to state and errors BEFORE calling session.start()
        val stateJob = scope.launch {
            session.state.collect { state ->
                when (state) {
                    DeviceSessionState.STARTED -> attachCapabilities(session)
                    DeviceSessionState.PAUSED -> { /* Temporarily suspended; wait for resume or stop */ }
                    DeviceSessionState.STOPPED -> cleanupSession()
                    else -> Unit
                }
            }
        }
        val errorJob = scope.launch {
            session.errors.collect { error: DeviceSessionError ->
                when (error) {
                    DeviceSessionError.DAT_APP_ON_THE_GLASSES_UPDATE_REQUIRED ->
                        Wearables.openDATGlassesAppUpdate(activity)
                    DeviceSessionError.DWA_OUT_OF_STU_RANGE ->
                        Log.w("DAT", "Non-blocking update suggestion: ${error.description}")
                    else -> Log.e("DAT", "Session error (${error.name}): ${error.description}")
                }
            }
        }
        session.start()
    },
    onFailure = { error, _ -> Log.e("DAT", "createSession failed: ${error.description}") }
)
```

---

## 4. Taking Pictures (In-Stream & Standalone High-Res)

The DAT SDK provides **two distinct photo capture paths** on `Camera`:

| Feature | 1. In-Stream Photo (`camera.stream.capturePhoto()`) | 2. Standalone Photo (`camera.photo.capturePhoto(...)`) |
|---|---|---|
| **Status** | Public Preview | Experimental |
| **Stream State** | Requires `camera.stream` to be actively `STREAMING` | Competes for camera hardware; `camera.stream` **must be stopped** first |
| **Output Type** | `DatResult<PhotoData, CaptureError>` (`PhotoData.Bitmap` or `PhotoData.HEIC`) | Asynchronous delivery via `photo.photoStream` (`PhotoCaptureData`) + `transferProgressStream` |
| **Knobs** | Uses active stream configuration | Configurable `PhotoResolution` (`SMALL`, `MEDIUM`, `LARGE`, `FULL`) & `PhotoQuality` (`LOW`, `MEDIUM`, `HIGH`) |

### 4.1 Path 1: In-Stream Photo Capture + EXIF Orientation Normalization
When capturing while video streaming is active, `camera.stream.capturePhoto()` is a `suspend` function returning `DatResult<PhotoData, CaptureError>`. Only one capture can be in flight at a time (`CaptureError.CaptureInProgress`).

**Important Android Quirk:** When `PhotoData.HEIC` is returned, `BitmapFactory.decodeByteArray` does **not** automatically apply the EXIF orientation tag embedded by the glasses, resulting in sideways images. Always parse `ExifInterface.TAG_ORIENTATION` and rotate the decoded `Bitmap`:

```kotlin
import androidx.exifinterface.media.ExifInterface
import com.meta.wearable.dat.camera.types.PhotoData
import java.io.ByteArrayInputStream
import java.nio.ByteBuffer

suspend fun captureInStreamPhoto(stream: Stream): Bitmap? {
    var output: Bitmap? = null
    stream.capturePhoto()
        .onSuccess { photoData ->
            output = withContext(Dispatchers.Default) {
                when (photoData) {
                    is PhotoData.Bitmap -> photoData.bitmap
                    is PhotoData.HEIC -> decodeHeicWithExif(photoData.data)
                }
            }
        }
        .onFailure { error, _ ->
            Log.e("DAT", "In-stream capture failed: ${error.description}")
        }
    return output
}

private fun decodeHeicWithExif(buffer: ByteBuffer): Bitmap? {
    val dup = buffer.duplicate().apply { rewind() }
    val bytes = ByteArray(dup.remaining()).also { dup.get(it) }
    val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return null

    val orientation = ByteArrayInputStream(bytes).use { input ->
        ExifInterface(input).getAttributeInt(
            ExifInterface.TAG_ORIENTATION,
            ExifInterface.ORIENTATION_NORMAL
        )
    }
    val matrix = Matrix().apply {
        when (orientation) {
            ExifInterface.ORIENTATION_ROTATE_90 -> postRotate(90f)
            ExifInterface.ORIENTATION_ROTATE_180 -> postRotate(180f)
            ExifInterface.ORIENTATION_ROTATE_270 -> postRotate(270f)
            ExifInterface.ORIENTATION_FLIP_HORIZONTAL -> postScale(-1f, 1f)
            ExifInterface.ORIENTATION_FLIP_VERTICAL -> postScale(1f, -1f)
            ExifInterface.ORIENTATION_TRANSPOSE -> { postRotate(90f); postScale(-1f, 1f) }
            ExifInterface.ORIENTATION_TRANSVERSE -> { postRotate(270f); postScale(-1f, 1f) }
        }
    }
    if (matrix.isIdentity) return bitmap
    return Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, matrix, true)
        .also { bitmap.recycle() }
}
```

### 4.2 Path 2: Standalone High-Resolution Capture (`camera.photo`)
Accessing `camera.photo` lazily activates the standalone photo child capability. Ensure `camera.stream` is stopped before accessing `camera.photo`.

```kotlin
import com.meta.wearable.dat.camera.types.PhotoQuality
import com.meta.wearable.dat.camera.types.PhotoResolution
import com.meta.wearable.dat.camera.types.PhotoState

fun setupStandalonePhotoCapture(session: DeviceSession) {
    // Attach camera without starting camera.stream
    session.addCamera(StreamConfiguration()).onSuccess { camera ->
        val photo = camera.photo // Lazily starts the Photo capability

        val stateJob = scope.launch {
            photo.state.collect { state: PhotoState ->
                if (state == PhotoState.STARTED) {
                    // Ready to trigger capture
                    photo.capturePhoto(
                        resolution = PhotoResolution.FULL, // SMALL (480p), MEDIUM (720p), LARGE (1080p), FULL (12MP)
                        quality = PhotoQuality.HIGH        // LOW, MEDIUM, HIGH
                    )
                }
            }
        }
        val progressJob = scope.launch {
            photo.transferProgressStream.collect { progress ->
                // Update transfer UI while high-res JPEG crosses Bluetooth Classic
            }
        }
        val dataJob = scope.launch {
            photo.photoStream.collect { capture ->
                val jpegBytes: ByteArray = capture.imageData
                saveJpeg(jpegBytes)
            }
        }
        val errorJob = scope.launch {
            photo.errors.collect { error ->
                Log.e("DAT", "Standalone photo error: ${error.description}")
            }
        }
    }
}
```
*Note: `photo.stop()` is terminal and `camera.photo` returns the same stopped instance. To re-enter standalone photo mode later, call `session.removeCamera()` and attach a new `Camera`.*

---

## 5. Video Streaming & Passthrough MP4 Recording

### 5.1 Configuring & Observing `camera.stream`
| Video Quality | Dimensions | Valid Frame Rates (`frameRate`) |
|---|---|---|
| `VideoQuality.LOW` | 360 × 640 | `2`, `7`, `15`, `24`, `30` FPS |
| `VideoQuality.MEDIUM` | 504 × 896 | `2`, `7`, `15`, `24`, `30` FPS |
| `VideoQuality.HIGH` | 720 × 1280 | `2`, `7`, `15`, `24`, `30` FPS |

- **Decoded YUV vs. Compressed HEVC:**
  - `StreamConfiguration(compressVideo = false)` (default): SDK decodes frames on the phone; `VideoFrame.buffer` contains raw YUV pixel data.
  - `StreamConfiguration(compressVideo = true)`: SDK delivers raw Annex-B HEVC NAL units (`frame.isCompressed == true`, `frame.isCodecConfig`). Use this when simultaneously rendering to a `Surface` and muxing passthrough MP4 video without re-encoding.
- **Stream States & Cap-Touch Pause:**
  - `STOPPED -> STARTING -> STARTED -> STREAMING -> STOPPING -> STOPPED` (and `CLOSED` when terminal).
  - A single tap on the glasses temple touchpad transitions the stream to `StreamState.PAUSED`. Do not tear down the stream on `PAUSED`; freeze the last frame and wait for another tap or resume.
  - Observe `camera.stream.errorStream`: `StreamError.STREAM_ERROR` is non-fatal/informational; `StreamError.CRITICAL_STREAM_ERROR` requires tearing down the stream.

### 5.2 Passthrough HEVC MP4 Recording Architecture
As demonstrated in the `CameraAccess` sample (`HevcParameterSetCollector`, `VideoCaptureHandler`, `HevcDecoder`), recording compressed HEVC streams reliably to MP4 requires handling four hardware/SDK behaviors:

1. **Cache HEVC Parameter Sets (`VPS` = NAL 32, `SPS` = NAL 33, `PPS` = NAL 34) from Stream Start:**
   The SDK emits the `VPS` once at stream start (sometimes prefixed with a double Annex-B start code `00 00 00 01 00 00 00 01`). If the user starts recording mid-stream, `MediaMuxer` will fail with `"Missing codec specific data"` unless you cache the latest VPS, SPS, and PPS NALs across every frame and prime `csd-0` when recording begins.
2. **Detect Any IRAP Keyframe (NAL Types `16..21`) + Grace Fallback:**
   Check for any HEVC IRAP NAL (`BLA` 16–18, `IDR` 19–20, `CRA` 21) to open the video track so playback starts cleanly. Include a 60-frame (~2s) fallback to open on any frame if using a mock/emulator source that omits IRAP NAL headers.
3. **Clamp Presentation Timestamps (`presentationTimeUs`) Across Pause/Resume:**
   When a stream pauses (`StreamState.PAUSED`) and resumes, incoming `presentationTimeUs` values can jump backward or stall, causing `MediaMuxer.writeSampleData` to throw `IllegalStateException`. Subtract the first frame's base timestamp and enforce a monotonic floor:
   ```kotlin
   var adjustedPts = frame.presentationTimeUs - videoBaseTimeUs
   if (lastWrittenPtsUs >= 0 && adjustedPts < lastWrittenPtsUs + FRAME_DURATION_US) {
       adjustedPts = lastWrittenPtsUs + FRAME_DURATION_US // e.g., 33_333L us
   }
   lastWrittenPtsUs = adjustedPts
   ```
4. **Software HEVC Decoder Preference for Live Preview:**
   Some hardware decoders (`OMX.Exynos.hevc.dec`, `c2.mtk.hevc.decoder`) corrupt glasses HEVC streams. When decoding `compressVideo = true` streams onto an `AndroidExternalSurface`, prefer a software HEVC decoder from `MediaCodecList(MediaCodecList.ALL_CODECS)` where `info.isSoftwareOnly` is true.

---

## 6. Hardware Inputs, Touchpad Gestures & IMU Software Macros

### 6.1 Hardware Inputs (`mwdat-inputs`)
`Inputs` delivers semantic interactions from the glasses temple touchpad, capture button, action button, and paired input accessories.
- **No runtime permission:** Requires WDC capability approval (`InputsError.PERMISSION_DENIED` means WDC approval is missing, not a runtime permission).
- **Auto-starting:** Calling `session.addInputs(...)` starts the capability automatically (there is no `inputs.start()`).
- **Back interception:** `InputsConfiguration(consumeBack = true)` (the default) intercepts the temple Back gesture so it is delivered as `InputEvent.Back` instead of the system exiting the experience.

```kotlin
import com.meta.wearable.dat.inputs.addInputs
import com.meta.wearable.dat.inputs.removeInputs
import com.meta.wearable.dat.inputs.types.*

fun attachGlassesInputs(session: DeviceSession) {
    session.addInputs(InputsConfiguration(consumeBack = true)).fold(
        onSuccess = { inputs ->
            val eventsJob = scope.launch {
                inputs.events.collect { event: InputEvent ->
                    when (event) {
                        is InputEvent.Capture -> when (event.pressType) {
                            CapturePressType.SHORT_PRESS -> triggerShutterMacro()
                            CapturePressType.HOLD -> startVoiceNoteMacro()
                            CapturePressType.DOUBLE_PRESS -> toggleDisplayOverlayMacro()
                        }
                        is InputEvent.Button -> when (event.button) {
                            ButtonType.ACTION -> triggerCustomActionMacro()
                            else -> Unit
                        }
                        is InputEvent.Nav -> when (event.direction) {
                            NavDirection.UP, NavDirection.RIGHT -> nextItemMacro()
                            NavDirection.DOWN, NavDirection.LEFT -> previousItemMacro()
                        }
                        is InputEvent.Select -> confirmSelectionMacro()
                        is InputEvent.Back -> dismissCurrentCardMacro()
                        is InputEvent.Drag -> handleContinuousScrub(
                            event.action, event.x, event.y, event.dx, event.dy
                        )
                    }
                }
            }
            val errorJob = scope.launch {
                inputs.errors.collect { error ->
                    error?.let { Log.e("DAT", "Inputs error: ${it.description}") }
                }
            }
        },
        onFailure = { error, _ -> Log.e("DAT", "addInputs failed: ${error.description}") }
    )
}
```
*Cleanup: Cancel collection jobs first, then call `session.removeInputs()`.*

### 6.2 Motion & IMU Software Macros (`mwdat-motion`)
`Motion` streams inertial samples (`accelerometer` in m/s², `gyroscope` in rad/s, `magnetometer` in µT, and `orientation` as a `Quaternion`) at `5`, `10` (default), `15`, `24`, `30`, or `60` Hz (`MotionSamplingRate`).

In `BirdSpotter` (`GlassesAim.kt`), `Motion` is used as a hands-free **head-aim macro** that automatically tags photos and detections with **Elevation Stratum** (`GROUND`, `UNDERSTORY`, `HORIZON`, `CANOPY`, `OVERHEAD`) and **Tilt-Compensated Magnetic Bearing**:

```kotlin
import com.meta.wearable.dat.motion.addMotion
import com.meta.wearable.dat.motion.removeMotion
import com.meta.wearable.dat.motion.types.*
import kotlin.math.asin
import kotlin.math.atan2

fun attachMotionMacro(session: DeviceSession) {
    session.addMotion(MotionConfiguration(samplingRate = MotionSamplingRate.HZ_30)).fold(
        onSuccess = { motion ->
            // Launch collectors BEFORE calling motion.start()
            val sampleJob = scope.launch {
                motion.samples.collect { sample: MotionSample ->
                    // Filter to glasses rigid body (ignore NEURAL_BAND or UNKNOWN if only head pose is wanted)
                    if (sample.source != MotionSource.GLASSES) return@collect
                    val accel = sample.accelerometer ?: return@collect

                    val elevationDeg = computeHeadElevationDegrees(accel)
                    val bearingDeg = sample.magnetometer?.let { mag ->
                        computeTiltCompensatedBearing(accel, mag)
                    }
                    onHeadAimUpdated(elevationDeg, bearingDeg)
                }
            }
            motion.start()
        },
        onFailure = { error, _ -> Log.e("DAT", "addMotion failed: ${error.description}") }
    )
}

// Measured body frame on worn glasses: +Y is sky/up reaction to gravity, -Z is forward gaze
private fun computeHeadElevationDegrees(accel: Vector3): Double? {
    val norm = Math.sqrt((accel.x * accel.x + accel.y * accel.y + accel.z * accel.z).toDouble())
    if (norm < 1e-6) return null
    val forwardDotUp = (-accel.z / norm).coerceIn(-1.0, 1.0)
    return Math.toDegrees(asin(forwardDotUp)) // >52.5° OVERHEAD, >17.5° CANOPY, >-17.5° HORIZON, >-52.5° UNDERSTORY, else GROUND
}
```
*Cleanup: Call `motion.stop()`, cancel collection jobs, and call `session.removeMotion()`.*

---

## 7. Voice Conversation, Audio Streaming & IVR Capabilities

A complete eyes-free/hands-free voice conversation and Interactive Voice Response (IVR) experience on Meta glasses combines five mechanisms:

### 7.1 "Hey Meta" Voice Invocations (`VoiceInvocationsStream`)
Lets the wearer launch or wake your app by saying *"Hey Meta, open/start {Spoken App Name}"* (configured in WDC).
- **Wearables-level stream:** Does **not** require a `DeviceSession` or camera/microphone permissions.
- **Cold & Warm Entry Points:** Check `isVoiceInvocationsIntent(intent)` in **both** `Activity.onCreate` (when `savedInstanceState == null`) and `Activity.onNewIntent`.
- **Mandatory Acknowledgement Contract:** Meta AI holds every invocation open waiting for a response. You **must** call `invocation.responseHandle.sendSuccess(null)` (or `sendFailure(...)`) **exactly once** as soon as the invocation arrives, or Meta AI announces that your app is not responding.
- **Pocketed / Locked-Screen Execution:** Because a voice launch often happens while the phone is locked in a pocket, call `setShowWhenLocked(true)` and `setTurnScreenOn(true)` in the Activity so Android permits starting a `microphone|connectedDevice` Foreground Service.

```kotlin
import com.meta.wearable.dat.core.voiceinvocations.isVoiceInvocationsIntent
import com.meta.wearable.dat.core.voiceinvocations.startVoiceInvocationsStream
import com.meta.wearable.dat.core.voiceinvocations.types.actions.LaunchApp

// In MainActivity:
override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (savedInstanceState == null && isVoiceInvocationsIntent(intent)) {
        handleHeyMetaLaunch()
    }
}

override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    setIntent(intent)
    if (isVoiceInvocationsIntent(intent)) {
        handleHeyMetaLaunch()
    }
}

private fun handleHeyMetaLaunch() {
    setShowWhenLocked(true)
    setTurnScreenOn(true)
    startRealtimeForegroundService()
    startGlassesExperience()
}

// App-scoped VoiceInvocationsStream (wait for RegistrationState.REGISTERED before opening):
suspend fun observeVoiceInvocations() {
    Wearables.registrationState.first { it == RegistrationState.REGISTERED }
    val stream = Wearables.startVoiceInvocationsStream(AutoDeviceSelector())
    try {
        stream.invocations.collect { invocation ->
            when (invocation) {
                is LaunchApp -> {
                    // 1. Acknowledge immediately so Meta AI doesn't time out
                    invocation.responseHandle.sendSuccess(actionOutput = null)
                    // 2. Trigger app experience
                    onVoiceLaunchRequested()
                }
            }
        }
    } finally {
        stream.close()
    }
}
```

### 7.2 On-Device Speech Recognition (`mwdat-speech`)
Streams real-time speech-to-text transcribed directly on the glasses (`TranscriptionResult` with `text`, `isFinal`, and `confidence` in `0f..1f` or `-1f` when unavailable).
- **Permissions:** Requires DAT `Permission.MICROPHONE` (requested via `Wearables.RequestPermissionContract()`). Does **not** require Android `RECORD_AUDIO` on real glasses (`RECORD_AUDIO` is only used if testing with `MockSpeechSource.LIVE_DEVICE_ASR`).
- **Coexistence:** Because transcription runs on the glasses and only text crosses the link, it does not contend with the phone's `AudioRecord`.

```kotlin
import com.meta.wearable.dat.speech.addSpeech
import com.meta.wearable.dat.speech.removeSpeech
import com.meta.wearable.dat.speech.types.SpeechState

fun attachSpeechRecognition(session: DeviceSession) {
    session.addSpeech().fold(
        onSuccess = { speech ->
            // Launch collectors BEFORE speech.start()
            val stateJob = scope.launch {
                speech.state.collect { state: SpeechState ->
                    // STARTING, STARTED (listening), PAUSED, STOPPING, STOPPED
                }
            }
            val transcriptJob = scope.launch {
                speech.transcriptions.collect { result ->
                    result ?: return@collect
                    val confidence = result.confidence.takeIf { it >= 0f }
                    if (result.isFinal) {
                        onFinalUserUtterance(result.text, confidence)
                    } else {
                        onPartialUserUtterance(result.text)
                    }
                }
            }
            val errorJob = scope.launch {
                speech.errors.collect { error ->
                    Log.e("DAT", "Speech error: ${error.description}")
                }
            }
            speech.start().onFailure { error, _ ->
                Log.e("DAT", "Failed to start speech: ${error.description}")
            }
        },
        onFailure = { error, _ -> Log.e("DAT", "addSpeech failed: ${error.description}") }
    )
}
```

### 7.3 Synchronized PCM Audio Streaming (`camera.stream.audioStream`)
When your app needs raw audio samples from the glasses microphones (e.g., for ambient sound classification, live sonograms, or streaming audio to a multimodal voice API):
- Require both `Permission.CAMERA` and `Permission.MICROPHONE`.
- Pass a non-null `AudioCodec.PCM` (`RATE_16000`, `RATE_44100`, or `RATE_48000`, `numberOfChannels = 1`) in `StreamConfiguration`:

```kotlin
import com.meta.wearable.dat.camera.types.AudioCodec
import com.meta.wearable.dat.camera.types.AudioSampleRate

val config = StreamConfiguration(
    audioCodec = AudioCodec.PCM(
        sampleRate = AudioSampleRate.RATE_16000,
        numberOfChannels = 1,
    ),
    videoQuality = VideoQuality.MEDIUM,
    frameRate = 24,
)

session.addCamera(config).onSuccess { camera ->
    // Collect audioStream on Dispatchers.IO BEFORE calling camera.stream.start()
    val audioJob = scope.launch(Dispatchers.IO) {
        camera.stream.audioStream.collect { audioFrame ->
            val pcmBuffer: ByteBuffer = audioFrame.buffer
            val ptsUs: Long = audioFrame.presentationTimeUs
            processPcmFrame(pcmBuffer, ptsUs)
        }
    }
    camera.stream.start()
}
```
*(Alternative for Production Channels: As shown in `CameraAccess`'s `AudioInputHandler.kt`, if experimental `AudioCodec.PCM` is not available on your release channel, you can route the glasses' HFP microphone via `AudioManager.setCommunicationDevice(scoDevice)` with `AudioDeviceInfo.TYPE_BLUETOOTH_SCO` and record using Android `AudioRecord`.)*

### 7.4 Spoken Output (TTS) & Acoustic Echo Suppression
The DAT SDK does not have a dedicated speaker API because glasses act as standard Bluetooth audio sinks (`TYPE_BLUETOOTH_A2DP`, `TYPE_BLUETOOTH_SCO`, `TYPE_BLE_HEADSET`, `TYPE_BLE_SPEAKER`). Use Android's `TextToSpeech` with two critical safeguards (implemented in `BirdSpotter`'s `SystemSpokenOutput.kt`):
1. **Bluetooth Route Gate:** Check `AudioManager.getDevices(AudioManager.GET_DEVICES_OUTPUTS)` before speaking so the phone never blasts TTS out of its loudspeaker in the field when glasses aren't routed.
2. **Echo / Self-Trigger Suppression (`isSpeaking`):** The glasses' speakers sit inches from the glasses' microphones. Expose an `isSpeaking` flag from your `UtteranceProgressListener` and pause/ignore incoming audio frames and ASR transcriptions while `isSpeaking == true` so the app never transcribes or classifies its own voice.

### 7.5 Visual & Voice IVR Menus on Display Glasses (`mwdat-display`)
On display-capable glasses (`device.isDisplayCapable()`, such as Meta Ray-Ban Display), you can pair spoken TTS prompts and voice commands with interactive heads-up display (HUD) cards and temple swipe/tap selection.

- **Device Selection:** Filter with `AutoDeviceSelector(filter = { it.isDisplayCapable() })` or `SpecificDeviceSelector(deviceId)`.
- **Attachment:** Call `session.addDisplay()` after `DeviceSessionState.STARTED`, and wait for `display.state` to reach `DisplayState.STARTED` before sending content.
- **Single Root Rule:** Every `display.sendContent { ... }` call replaces the entire screen and must have **exactly one root element**: either a root `flexBox { ... }` or a root `video(player = player)`. Never nest `video(...)` inside a `flexBox`.
- **Interactive IVR Routing:** `button(...)` and clickable `flexBox(..., onClick = { ... })` callbacks execute on the phone. Use `ActionRole.PRIMARY` on the default button so it receives initial focus on the glasses temple controller.

```kotlin
import com.meta.wearable.dat.display.Display
import com.meta.wearable.dat.display.addDisplay
import com.meta.wearable.dat.display.removeDisplay
import com.meta.wearable.dat.display.types.*
import com.meta.wearable.dat.display.views.*

suspend fun sendIvrMenuCard(
    display: Display,
    title: String,
    subtitle: String,
    thumbnailBitmap: Bitmap?,
    onConfirm: () -> Unit,
    onWatchVideo: () -> Unit,
    onBack: () -> Unit,
) {
    display.sendContent {
        flexBox(direction = Direction.COLUMN, gap = 12) {
            flexBox(padding = 24, background = FlexBoxBackground.CARD) {
                if (thumbnailBitmap != null) {
                    image(
                        bitmap = thumbnailBitmap, // Scale bitmaps to <= 600x600 px before sending over BT
                        sizePreset = ImageSize.FILL,
                        cornerRadius = CornerRadius.MEDIUM,
                    )
                }
                text(title, style = TextStyle.HEADING)
                text(subtitle, style = TextStyle.META, color = TextColor.SECONDARY)
            }

            flexBox(
                direction = Direction.ROW,
                gap = 8,
                alignment = Alignment.CENTER,
                crossAlignment = Alignment.CENTER,
            ) {
                buttonGroup {
                    button(
                        label = "Back",
                        style = ButtonStyle.PRIMARY,
                        iconName = IconName.TRIANGLE_LEFT_VERTICAL_LINE,
                        onClick = onBack,
                    )
                    button(
                        label = "Confirm",
                        style = ButtonStyle.PRIMARY,
                        iconName = IconName.CHECKMARK,
                        actionRole = ActionRole.PRIMARY, // Receives default focus on glasses
                        onClick = onConfirm,
                    )
                    button(
                        label = "Watch video",
                        style = ButtonStyle.SECONDARY,
                        iconName = IconName.VIDEO_CAMERA,
                        onClick = onWatchVideo,
                    )
                }
            }
        }
    }.onFailure { error, _ ->
        Log.e("DAT", "Display sendContent failed: ${error.description}")
    }
}
```

#### Playing MP4 Video on the Glasses Display
```kotlin
fun playHudVideo(display: Display, mp4Url: String, onEnded: () -> Unit) {
    val player = VideoPlayer(
        source = VideoSource.Url(mp4Url), // Must be faststart MP4 within display player limits
        codec = VideoCodec.MP4,
    )
    val stateJob = scope.launch {
        player.state.collect { state ->
            if (state == VideoPlayerState.ENDED) onEnded()
        }
    }
    val errorJob = scope.launch {
        player.error.collect { error ->
            error?.let { Log.e("DAT", "VideoPlayer error: ${it.description}") }
        }
    }
    scope.launch {
        display.sendContent { video(player = player) }
            .onSuccess { player.play() }
            .onFailure { error, _ -> Log.e("DAT", "Video send failed: ${error.description}") }
    }
}
```
*Cleanup: Call `display.clearDisplay()` if clearing the HUD, cancel state/video jobs, call `session.removeDisplay()`, then `session.stop()`.*

---

## 8. VLM / AI Tool-Calling & Multimodal Pipeline Architecture

The DAT SDK is a **hardware and sensor transport toolkit**—it does not bundle an on-device LLM or VLM. Instead, it is designed to feed multimodal AI / VLM tool-calling pipelines (or deterministic demo directors like `BirdSpotter`'s `PresetDemoDirector`).

### Recommended VLM / Tool-Calling Wiring Pattern
When integrating an external VLM/LLM (e.g., Llama, OpenAI Realtime, Gemini Live) with DAT on Android, expose the glasses capabilities as **asynchronous tools** and **event streams** coordinated by a session ViewModel:

```text
[Glasses Hardware]
   ├── Speech (on-device ASR) ──(isFinal transcript)──► [VLM / Agent Orchestrator]
   ├── Inputs (Temple / Capture) ──(SHUTTER / ACTION)─►   ├── Tool: capture_visual_context()
   ├── Motion (IMU 30Hz) ─────────(Elevation/Bearing)─►   ├── Tool: render_hud_card(title, body, image)
   ├── Camera (Stream / Photo) ◄──(capturePhoto)──────┘   └── Tool: speak_response(tts_text)
   └── Display + BT Audio ◄───────(sendContent + TTS)──────────┘
```

```kotlin
class MultimodalAgentCoordinator(
    private val session: DeviceSession,
    private val camera: Camera,
    private val display: Display?,
    private val spokenOutput: SpokenOutput,
) {
    @Volatile var latestHeadElevationDeg: Double? = null
    @Volatile var latestCompassBearingDeg: Double? = null

    // 1. Triggered when Speech emits a final transcript: speech.transcriptions.collect { ... }
    suspend fun onUserSpokeFinal(utterance: String) {
        if (spokenOutput.isSpeaking) return // Ignore self-echo

        // Send user utterance + current head-aim metadata to VLM / Tool-Calling API
        val response = vlmClient.chatWithTools(
            userText = utterance,
            metadata = mapOf(
                "head_elevation_deg" to latestHeadElevationDeg,
                "compass_bearing_deg" to latestCompassBearingDeg,
            ),
            onToolCall = ::executeGlassesToolCall
        )

        // Deliver response via HUD Display and Bluetooth TTS
        response.displayCard?.let { card ->
            display?.sendContent {
                flexBox(direction = Direction.COLUMN, gap = 12, padding = 16) {
                    text(card.title, style = TextStyle.HEADING)
                    card.bitmap?.let { image(bitmap = it) }
                    text(card.body, style = TextStyle.BODY)
                }
            }
        }
        response.spokenReply?.let { line ->
            spokenOutput.speak(line)
        }
    }

    // 2. VLM Tool Executor: allows the model to snap a photo from the wearer's perspective on demand
    private suspend fun executeGlassesToolCall(toolName: String, args: Map<String, Any?>): String {
        return when (toolName) {
            "capture_glasses_photo" -> {
                val bitmap = captureInStreamPhoto(camera.stream)
                    ?: return """{"error": "capture_failed"}"""
                val visionAnalysis = vlmClient.analyzeImage(
                    bitmap = bitmap,
                    elevationDeg = latestHeadElevationDeg,
                    bearingDeg = latestCompassBearingDeg,
                )
                visionAnalysis
            }
            "clear_glasses_display" -> {
                display?.clearDisplay()
                """{"status": "cleared"}"""
            }
            else -> """{"error": "unknown_tool"}"""
        }
    }
}
```

---

## 9. Testing with MockDeviceKit & Display Simulators

`mwdat-mockdevice` lets you test every capability—including registration, permissions, video/photo feeds, cap-touch/input gestures, ASR speech, IMU poses, voice launches, and the Ray-Ban Display HUD—without physical glasses.

### 9.1 Pairing & Controlling Simulated Glasses
```kotlin
import com.meta.wearable.dat.mockdevice.MockDeviceKit
import com.meta.wearable.dat.mockdevice.api.GlassesModel
import com.meta.wearable.dat.mockdevice.api.MockDeviceKitConfig
import com.meta.wearable.dat.mockdevice.api.camera.CameraFacing
import com.meta.wearable.dat.mockdevice.api.speech.MockSpeechSource

val mockKit = MockDeviceKit.getInstance(context)

// Enable with auto-registration and permissions granted (or false to test onboarding flows)
mockKit.enable(MockDeviceKitConfig(initiallyRegistered = true, initialPermissionsGranted = true))

// Pair any supported model:
// RAYBAN_META, OAKLEY_META_HSTN, OAKLEY_META_VANGUARD, RAYBAN_META_OPTICS, META_GLASSES, META_RAYBAN_DISPLAY
val glasses = mockKit.pairGlasses(GlassesModel.META_RAYBAN_DISPLAY).getOrNull()!!

// 1. Hardware state & telemetry
glasses.powerOn()
glasses.unfold()
glasses.don()
glasses.setBatteryLevel(85)
glasses.setChargingState(ChargingState.NOT_CHARGING)
glasses.setThermalLevel(ThermalLevel.NONE)

// 2. Camera feed & still photo injection
// Note: CameraFacing requires android.permission.CAMERA granted on the phone!
glasses.services.camera.setCameraFeed(CameraFacing.BACK) // or setCameraFeed(hevcVideoUri)
glasses.services.camera.setCapturedImage(jpegUri)        // For stream.capturePhoto()
glasses.services.cameraCapture.setCapturedPhoto(jpegUri) // For standalone camera.photo.capturePhoto()

// 3. Cap-touch & Inputs injection (inject only after Inputs capability is active)
glasses.services.captouch.tap()        // Pauses/resumes active camera stream
glasses.services.captouch.tapAndHold() // Stops active camera stream
glasses.services.input.capture(CapturePressType.SHORT_PRESS)
glasses.services.input.button(ButtonType.ACTION)
glasses.services.input.navDown(InputSource.CAPTOUCH)
glasses.services.input.select(InputSource.CAPTOUCH)
glasses.services.input.back(InputSource.CAPTOUCH)

// 4. Speech ASR injection (inject after Speech is listening)
glasses.services.speech.setTranscriptionSource(MockSpeechSource.INJECTED) // or LIVE_DEVICE_ASR
glasses.services.speech.simulateTranscription("what bird is that", isFinal = false, confidence = 0.8f)
glasses.services.speech.simulateTranscription("what bird is that", isFinal = true, confidence = 0.95f)

// 5. Motion IMU feed injection
glasses.services.motion.setMotionFeed(samples = listOf(
    MotionSample(
        timestampNs = 0L,
        accelerometer = Vector3(9.81f, 0f, 0f),
        gyroscope = Vector3(0f, 0f, 0f),
        magnetometer = null,
        orientation = null,
        source = MotionSource.GLASSES,
    )
), loop = true)

// 6. Voice Invocation ("Hey Meta") simulation
if (glasses.services.voiceInvocation.hasConnectedApps()) {
    glasses.services.voiceInvocation.simulateLaunchAppAction()
}
```

### 9.2 In-App & Chrome Display Simulators (`META_RAYBAN_DISPLAY`)
When paired with `GlassesModel.META_RAYBAN_DISPLAY`:
- **In-App Phone Preview:** Render the live 1:1 glasses HUD directly inside your Compose UI:
  ```kotlin
  AndroidView(
      factory = { ctx -> glasses.services.display.createPreviewView(ctx) },
      modifier = Modifier.fillMaxWidth().aspectRatio(1f)
  )
  // Programmatically click a display element by ID in tests:
  glasses.services.display.sendClick(identifier)
  ```
- **Chrome Simulator Extension Preview:** Start the built-in local HTTP test server and forward the port over ADB:
  ```kotlin
  mockKit.startTestServer(port = 8237).onSuccess { port ->
      // 1. Run on host machine: adb forward tcp:8237 tcp:8237
      // 2. Open http://127.0.0.1:8237/ in Chrome with the Meta Ray-Ban Display Simulator extension enabled
  }
  ```

### 9.3 Switching Between MockDeviceKit and Real Glasses Safely
Swapping `MockDeviceKit.enable()` / `disable()` replaces the SDK's underlying providers live, but does **not** automatically close active leases. Always:
1. Stop any active `DeviceSession` and wait for `DeviceSessionState.STOPPED`.
2. Close any active `VoiceInvocationsStream`.
3. Call `mockKit.stopTestServer()` and `mockKit.disable()` (or `enable()`).

---

## 10. Common Pitfalls & Troubleshooting Checklist

1. **Calling `Wearables.initialize(context)` before `BLUETOOTH_CONNECT` is granted:** Causes the SDK to fail its initial bonded-device scan and remain `UNAVAILABLE` until app restart.
2. **Calling `session.addCamera(...)` / `addDisplay()` / `addSpeech()` immediately after `session.start()`:** Fails with `DeviceSessionError.SESSION_IDLE`. Always wait for `session.state` to emit `DeviceSessionState.STARTED`.
3. **Using `getOrElse` on `DatResult`:** `getOrElse` receives a raw `Throwable`, not the typed `DatError`. Always use `.fold(onSuccess = ..., onFailure = { error, cause -> ... })` or the two-parameter `.onFailure { error, _ -> }` to access `error.description`.
4. **Reusing stopped instances:** `DeviceSession.stop()`, `Camera.stop()`, `Stream.stop()`, `Photo.stop()`, and `VoiceInvocationsStream.close()` are terminal. Create a new instance (or call `session.removeCamera()` before `session.addCamera(...)`) for subsequent runs.
5. **Running `camera.stream` and `camera.photo` simultaneously:** They compete for the camera hardware. Stop `camera.stream` before accessing `camera.photo`.
6. **Forgetting `ResponseHandle.sendSuccess` on Voice Invocations:** Meta AI will hang and announce that the app is not responding. Acknowledge every `LaunchApp` invocation immediately upon receipt.
```
