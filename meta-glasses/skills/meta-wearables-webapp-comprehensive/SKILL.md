```markdown
---
name: meta-wearables-webapp-comprehensive
description: Complete developer reference for the Meta Wearables Web App AI Toolkit (v128.1.1) and WebApp Game Plugin (v1.0.2) for Meta Ray-Ban Display (MRBD) glasses. Covers project scaffolding, UI Toolkit for Meta Ray-Ban Display (IVR/D-pad menus, components, layout contracts), WebMCP agentic tool-calling & voice conversation ("Hey Meta"), hardware inputs/software macros (EMG pinch, continuous drag, D-pad, sensors, voice/handwriting text composer), camera/media & asset constraints, audio engine & banks, CDP performance profiling, deterministic QA gates, and Vercel/single-file deployment.
---

# Meta Wearables Web App & Game Toolkit — Master Skill

## 1. Platform Overview & Plugin Architecture

Web Apps for **Meta Ray-Ban Display (MRBD)** glasses are standard HTML/CSS/JavaScript applications rendered on a **600×600dp, 30 Hz additive waveguide display** controlled via **Captouch D-pad** (temple swipes) and **Neural Band EMG** (wristband thumb-to-index gestures).

The repository (`https://github.com/meta/meta-wearables-webapp`) is organized into two companion plugins and a live documentation MCP server:

| Component | Version | Stack | Purpose |
|---|---|---|---|
| `meta-wearables-webapp` | `128.1.1` | React 19 + Vite 8 + TypeScript + `@wearables-ui-toolkit/mrbd` | Standard interactive apps, dashboards, utilities, sensor tools, and WebMCP voice-agent experiences |
| `meta-wearables-webapp-game` | `1.0.2` | Vite 8 + TypeScript + Three.js (`0.184`) + Vitest + i18next | Real-time 2D (orthographic) and 3D (perspective) games with renderer/input/audio-agnostic architecture |
| **Live Docs MCP** | Public | HTTP MCP (`https://mcp.developer.meta.com/wearables`) | Query live Web Apps docs via `search_webapps_docs` (no auth/OAuth/tokens required) |

### 1.1 Capability Matrix: WebApp vs. Native DAT SDK (Camera, Video, Voice, Macros, IVR)

Because MRBD Web Apps run inside an on-glasses Chromium WebView rather than a companion phone app over Bluetooth/Wi-Fi, capabilities map differently than the native iOS/Android DAT SDK:

| Capability | WebApp Implementation & Boundaries |
|---|---|
| **Taking Pictures & Videos** | **Browser Media / Preloaded Assets:** Web Apps do not have a proprietary `MWDATCamera` SDK module. Apps that require live media capture must use standard Web APIs (`navigator.mediaDevices.getUserMedia` for `audioCapture` / `videoCapture`—supported in the CDP harness via `--grant-media`). For display/playback, images must be downscaled to rendered box size (`WebP` q75–80) and games must preload all textures/models up front via `preloadManifest` (`sealAssetLoaders()` / `sealAssetNetwork()`). |
| **Voice Conversation & VLM Tool Calls** | **WebMCP (`document.modelContext`) & Native Composer:** Instead of managing raw ASR/TTS streams, Web Apps register structured tools on `document.modelContext`. The wearer speaks naturally to **Meta AI** (*"Hey Meta, add coffee and sugar to my cart"*), Meta AI invokes your app's `execute` callback, and speaks the response based on your returned JSON `next_action`. For direct text entry, focusing and activating a standard `<input>` or `<textarea>` opens the glasses' native **voice + handwriting composer**. |
| **Software Macros & Hardware Inputs** | **EMG Neural Band + D-Pad + Web Sensors:** Discrete select (`Enter` / `onClick` / `pinchTap`), directional navigation (`ArrowUp`/`Down`/`Left`/`Right` / `dpadSwipe`), opt-in continuous relative pointer drag (`touch-action: none` + `PointerEvents` `movementX`/`movementY`), and multi-step **WebMCP macro tools** (collapsing compound voice intents into a single state transaction). Motion/orientation (`DeviceMotionEvent`, `DeviceOrientationEvent`) and GPS (`navigator.geolocation.watchPosition`) run via standard DOM APIs. |
| **Display & IVR Menus** | **UI Toolkit (`@wearables-ui-toolkit/mrbd`) & DOM HUD:** Interactive menu trees, action rails, scrollable lists, and carousels are built with `@wearables-ui-toolkit/mrbd` (`WearablesApp`, `Page`, `ScrollView`, `Panel`, `ButtonRail`, `Button`). In games, 2D/3D worlds render in WebGL (`<canvas>`) while all HUDs, menus, and text render in a crisp DOM overlay (`#hud`). |

---

## 2. Display Physics & Hardware Envelope

Every Web App and Game must be engineered for the physical realities of the glasses:

| Constraint | Specification | Engineering Rule |
|---|---|---|
| **Display Type** | 600×600dp, DPR 1, **Additive Waveguide** | Emitted light adds to the real world. **`#000000` emits no light and is 100% transparent.** `#FFFFFF` is maximum brightness. |
| **Window vs. Surface Colors** | Window: `var(--uit-color-background-window)` or `#000000`<br>Bounded Cards/Modals: `#0a0a0f` – `#1C1E21` | Keep the page background black/window-token so the wearer sees through the glasses. Bounded UI surfaces (cards, modals, panels) must use dark gray so they read as opaque. **Always-on game HUDs must have no background fill** (bright text directly over transparent black). |
| **Panel Refresh Rate** | **30 Hz** (33.3 ms frame budget) | Never run a 60 fps animation loop—it wastes battery for zero visual gain. Throttle sensor/UI React state commits to 10–30 Hz. Stop all loops/timers when hidden (`visibilitychange`). |
| **Network Link** | ~500 Kbps down (**1 KB ≈ 16 ms**), ~150 ms RTT | Total first-load transfer **< 300 KB** (React apps) / **< 500 KB gzipped** (Three.js games). Fewer than **15 initial requests** (apps) / **< 10 requests** (games). |
| **CPU & Memory** | ~12× slower than laptop core; **< 128 MB JS Heap** | Keep bundles small, inline critical HTML/CSS for `< 1s` first paint, and cap decoded Web Audio PCM at **24 MB** (`AUDIO.maxResidentBytes`). |

### 2.1 Required HTML Metadata
Every app and game `index.html` **must** include `<meta name="mrbd-web-app-capable" content="yes">`. Without it, the glasses will not route D-pad or EMG wristband input to the page.

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <!-- React UI Toolkit apps use responsive device-width: -->
    <meta name="viewport" content="width=device-width, initial-scale=1.0, viewport-fit=cover" />
    <!-- Custom opt-out apps and Three.js games use fixed 600x600: -->
    <!-- <meta name="viewport" content="width=600, height=600, initial-scale=1.0" /> -->

    <!-- Must be app-specific; starter placeholders fail the test gate -->
    <meta name="description" content="5-day weather forecast and radar for Meta Ray-Ban Display." />
    <!-- REQUIRED: Enables D-pad and EMG Neural Band input routing on glasses -->
    <meta name="mrbd-web-app-capable" content="yes" />
    <link rel="icon" href="data:," />
    <title>Glasses Weather</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

---

## 3. UI Toolkit & Interactive Menus (IVR / D-Pad Navigation)

Standard web apps use **UI Toolkit for Meta Ray-Ban Display** (`@wearables-ui-toolkit/mrbd` and `@wearables-ui-toolkit/icons`, documentation at `https://github.com/meta/meta-ray-ban-display-ui-toolkit-web/`).

### 3.1 Scaffolding & Installing UI Toolkit
Never scaffold into a non-empty directory (except `.git`, `.claude`, `.cursor`, `.codex`, `.agents`). Never use `file:` dependencies, source aliases, or legacy `@meta/wearables-ui-*` packages.

```bash
# 1. Install the official UI Toolkit LLM skills once at account scope:
node plugins/meta-wearables-webapp/skills/ai-glasses-webapp-ui/scripts/install-ui-toolkit-skills.mjs \
  --client claude-code --account

# 2A. Initialize a brand-new React + Vite + TS + UI Toolkit app:
node plugins/meta-wearables-webapp/skills/ai-glasses-webapp-ui/scripts/init-webapp.mjs ./my-glasses-app

# 2B. Or add UI Toolkit idempotently to an existing React app:
node plugins/meta-wearables-webapp/skills/ai-glasses-webapp-ui/scripts/install-ui-toolkit.mjs ./existing-app

# 3. Run the bundled Apache-2.0 UI Toolkit structure validator:
node plugins/meta-wearables-webapp/skills/ai-glasses-webapp-ui/scripts/validate-ui-toolkit.mjs ./my-glasses-app
```

### 3.2 Viewport & CSS Contract (`src/styles.css`)
UI Toolkit apps **must** fill `100%` of the viewport using shrinkable flex/grid tracks and never hardcode `600px`:

```css
:root {
  color-scheme: dark;
}

html,
body,
#root {
  width: 100%;
  height: 100%;
  margin: 0;
  background: var(--uit-color-background-window);
}

body {
  overflow: hidden;
}

* {
  box-sizing: border-box;
}

.action-page-shell {
  display: grid;
  grid-template-rows: minmax(0, 1fr) auto;
  block-size: 100%;
  min-block-size: 0;
  min-inline-size: 0;
}

.action-dock {
  min-inline-size: 0;
  padding-block-end: var(--uit-spacing-xsmall);
}

.content-inset {
  display: flex;
  flex-direction: column;
  gap: calc(var(--uit-spacing-large) + var(--uit-spacing-xsmall));
  min-inline-size: 0;
  padding: var(--uit-spacing-large);
}
```

### 3.3 Composing Screens, Menus, and Action Docks (`src/App.tsx`)
Follow these strict composition boundaries (enforced by `validate-ui-toolkit.mjs`):
1. **Wrap in `<App>` (`WearablesApp`):** It loads the Toolkit stylesheet automatically. **Never** manually import `@wearables-ui-toolkit/mrbd/styles.css`.
2. **One vertical scroll owner per route:** Never put `ListItem` directly inside `ScrollView`, and never nest `VerticalList` inside `ScrollView`.
3. **Direct Panel child:** When using a page-level `<Panel width="100%">`, place it directly inside `<ScrollView>`.
4. **Sibling Bottom Dock:** Place page-level commands in a sibling bottom `<ButtonRail>` or `<ButtonGroup>` dock outside the `<ScrollView>`.
5. **No In-App Back Button:** The wearer uses the hardware/EMG back gesture (`Escape`). Restore focus and scroll position when returning from a sub-route or modal.
6. **Typography (`TextStyle`):** Valid enum members are `NUMERAL1`, `NUMERAL2`, `DISPLAY1`, `HEADING1`, `HEADING2`, `BODY1`, `BODY1_EMPHASIZED`, `BODY2`, `BODY2_EMPHASIZED`, `LABEL`, `LABEL_EMPHASIZED`, `META1`, `META1_EMPHASIZED`, `META2`, `META2_EMPHASIZED`, and `META3`. Keep routine product copy at `BODY2` or below.

```tsx
import { useState } from 'react';
import {
  App as WearablesApp,
  Button,
  ButtonRail,
  Page,
  Panel,
  ScrollView,
  TextStyle,
  TextView,
} from '@wearables-ui-toolkit/mrbd';

export default function App() {
  const [mode, setMode] = useState<'idle' | 'recording' | 'paused'>('idle');
  const [laps, setLaps] = useState<string[]>([]);

  return (
    <WearablesApp>
      <Page headerText="Field Recorder" enableSystemBarInset={false}>
        <div className="action-page-shell">
          <ScrollView insetForHeader ariaLabel="Session status and lap log" tabIndex={0}>
            <Panel width="100%">
              <div className="content-inset">
                <TextView as="p" textStyle={TextStyle.BODY2_EMPHASIZED}>
                  {mode === 'idle' ? 'Ready to capture.' : `Status: ${mode.toUpperCase()}`}
                </TextView>
                <TextView as="p" textStyle={TextStyle.BODY2}>
                  {laps.length === 0
                    ? 'Use D-pad to focus controls and pinch to activate.'
                    : `Recorded markers: ${laps.join(', ')}`}
                </TextView>
              </div>
            </Panel>
          </ScrollView>
          <div className="action-dock">
            <ButtonRail>
              <Button
                title={mode === 'recording' ? 'Pause' : 'Start'}
                onClick={() => setMode(m => (m === 'recording' ? 'paused' : 'recording'))}
              />
              <Button
                title="Mark"
                onClick={() => setLaps(prev => [...prev, `M${prev.length + 1}`])}
              />
            </ButtonRail>
          </div>
        </div>
      </Page>
    </WearablesApp>
  );
}
```

*(Note: If the user explicitly opts out of the UI Toolkit via `init-webapp.mjs <dir> --no-ui-toolkit`, `wearables.config.json` sets `"ui": "custom-explicit-opt-out"`, and `#root` must instead be styled as a fixed `600px × 600px` canvas centered inside `body { display: grid; place-items: center; }`.)*

---

## 4. Voice Conversation, VLM / Agent Tool Calling (WebMCP) & Text Input

Web Apps support two voice/assistant mechanisms:
1. **WebMCP (`document.modelContext`):** Registers agentic tools so the wearer can converse with **Meta AI** (*"Hey Meta..."*) to inspect or drive app state hands-free, including multi-step software macros.
2. **On-Glasses Composer (`<input>` / `<textarea>`):** Standard HTML text fields open the system handwriting + voice dictation UI when focused and activated.

### 4.1 WebMCP Architecture & Constraints (`ai-glasses-webapp-webmcp`)
The glasses browser injects `document.modelContext` before page scripts run. **Never install a WebMCP polyfill**, as it can shadow `document.modelContext` and prevent Meta AI from seeing your tools.

Key protocol rules:
- **Eligibility:** Decided once at app launch (requires Developer Mode or rollout). Relaunch the app after enabling.
- **Additive Only:** The app must remain 100% operable via D-pad without the assistant.
- **Always Include a Read-Only Orientation Tool:** Provide a tool marked `annotations: { readOnlyHint: true }` that returns current state so Meta AI can orient itself before mutating state.
- **One Utterance = One Tool Call (Software Macros):** If a user says *"Add coffee, sugar, and cream to my cart"*, handle the entire batch in a single tool call rather than 3 round-trips.
- **Scalar Parameters Only:** The bridge only carries `boolean`, `integer`, `number`, and `string`. `array` and `object` parameter types are JSON-encoded strings that fail if malformed. **Prefer comma- or newline-delimited `string` parameters** for lists and parse them defensively in `execute`.
- **Schema Limits:** Only top-level `required` and each property's `type` and `description` are read (`enum`, `minimum`, `maximum`, `default`, and `items` are stripped—state constraints inside `description`). Keep parameter descriptions **< 256 chars** and tool descriptions **< 1024 chars**.
- **Tool Naming:** Format as `appprefix_verb_noun`. Max **57 characters** (Meta AI prefixes tool names inside a 64-char budget; names sharing the first 57 chars collide).
- **Reserved Names (Silently Rejected!):** Never name a tool `openUrl`, `goBack`, `goForward`, `reload`, `getCurrentUrl`, `getPageTitle`, or `getPageText`.
- **10-Second Execution Deadline:** Forward the `{ signal }` argument to any `fetch` or async operation.
- **No `isError` Flag:** Fulfilled promises are always treated as success (`isError` is stripped). For handled domain errors (e.g., unknown item), return a normal JSON string `{ error: true, message, next_action }` so Meta AI can explain and recover. Only `throw` / reject for unrecoverable failures or cancellation.
- **Speech Ends the Turn:** Meta AI closes its turn as soon as it speaks. Always end your returned JSON with an explicit `next_action` (e.g., `"Confirm in one short sentence. Stop and talk."` or `"Now call cart_checkout."`).
- **Head Guardrails:** Meta AI always reads `<title>` and `<meta name="description">`. Put critical behavioral guardrails (e.g., *"Never complete a purchase without explicit confirmation"*) in `<meta name="description">` and tool descriptions.

### 4.2 Complete WebMCP Implementation Pattern

```tsx
import { useEffect } from 'react';

interface ModelContextTool {
  name: string;
  description: string;
  inputSchema: {
    type: 'object';
    properties: Record<string, { type: 'string' | 'number' | 'integer' | 'boolean'; description: string }>;
    required?: string[];
  };
  annotations?: { readOnlyHint?: boolean };
  execute: (args: Record<string, unknown>, context: { signal?: AbortSignal }) => Promise<unknown> | unknown;
}

interface ModelContext {
  registerTool: (tool: ModelContextTool, options?: { signal?: AbortSignal }) => unknown;
  getTools?: () => Promise<ModelContextTool[]>;
}

declare global {
  interface Document {
    modelContext?: ModelContext;
  }
}

// Helper for handled domain errors: Meta AI reads this JSON and follows next_action
function problem(message: string, next_action: string): string {
  return JSON.stringify({ error: true, message, next_action });
}

// Defensive list parser: handles JSON arrays, newline-separated, or comma-separated strings
export function splitList(input: unknown): string[] {
  let value = input;
  if (typeof value === 'string' && value.trim().startsWith('[')) {
    try {
      const parsed: unknown = JSON.parse(value.trim());
      if (Array.isArray(parsed)) value = parsed;
    } catch {
      /* fall through to delimiter split */
    }
  }
  if (Array.isArray(value)) {
    return value.map(p => String(p ?? '').trim()).filter(Boolean);
  }
  const text = String(value ?? '').trim();
  const chunks = text.includes('\n') ? text.split('\n') : text.split(',');
  return chunks.map(c => c.trim()).filter(Boolean);
}

// Guarded registration: registerTool may throw synchronously OR return a rejecting Promise OR return void
export function safeRegister(ctx: ModelContext, tool: ModelContextTool, signal?: AbortSignal): void {
  try {
    const result = ctx.registerTool(tool, signal ? { signal } : undefined);
    if (typeof (result as PromiseLike<unknown>)?.then === 'function') {
      (result as PromiseLike<unknown>).then(undefined, (err: unknown) =>
        console.error(`${tool.name} registration failed`, err),
      );
    }
  } catch (err) {
    console.error(`${tool.name} registration failed`, err);
  }
}

// Register tools sharing the same state mutators as the D-pad UI
export function registerCartTools(
  ctx: ModelContext,
  api: {
    getCart: () => string[];
    getTotal: () => number;
    menuNames: () => string[];
    isKnownItem: (name: string) => boolean;
    addItems: (names: string[]) => void;
    clearCart: () => void;
  },
  signal?: AbortSignal,
): void {
  // 1. Read-only state orientation tool
  safeRegister(
    ctx,
    {
      name: 'cart_get_state',
      description: 'Read current cart items, total price, and available menu items.',
      annotations: { readOnlyHint: true },
      inputSchema: { type: 'object', properties: {} },
      execute: async () =>
        JSON.stringify({
          cart: api.getCart(),
          total: api.getTotal(),
          availableMenu: api.menuNames(),
          next_action: 'Answer the wearer briefly or call the appropriate mutation tool.',
        }),
    },
    signal,
  );

  // 2. Multi-item macro mutation tool
  safeRegister(
    ctx,
    {
      name: 'cart_add_items',
      description:
        'Add one or more items to the cart in a single call. Returns updated cart and total.',
      inputSchema: {
        type: 'object',
        properties: {
          items: {
            type: 'string',
            description: 'Comma-separated item names, e.g. "coffee, sugar, cream".',
          },
        },
        required: ['items'],
      },
      execute: async ({ items }) => {
        const names = splitList(items);
        if (names.length === 0) {
          return problem('No items provided.', `Ask the wearer which items to add from: ${api.menuNames().join(', ')}.`);
        }
        const unknown = names.filter(n => !api.isKnownItem(n));
        if (unknown.length > 0) {
          return problem(
            `Not on the menu: ${unknown.join(', ')}.`,
            `Offer these valid items and call again: ${api.menuNames().join(', ')}.`,
          );
        }
        api.addItems(names); // Triggers React re-render so the HUD stays in sync
        return JSON.stringify({
          added: names,
          cart: api.getCart(),
          total: api.getTotal(),
          next_action: 'Confirm the additions in one short sentence. Stop and talk.',
        });
      },
    },
    signal,
  );
}

// React lifecycle hook
export function useCartWebMCP(api: Parameters<typeof registerCartTools>[1]): void {
  useEffect(() => {
    const ctx = document.modelContext;
    if (!ctx?.registerTool) return; // Surface absent: D-pad UI continues working normally
    const ac = new AbortController();
    registerCartTools(ctx, api, ac.signal);
    return () => ac.abort();
  }, [api]);
}
```

### 4.3 Voice & Handwriting Text Input
To let the wearer dictate or write freeform text:
- Render a standard `<input type="text">`, `<input type="search">`, or `<textarea>` with an explicit label or `aria-label`.
- When the wearer navigates to the field via D-pad and **activates it with a pinch/Enter**, the glasses OS opens the native **Voice + Handwriting Composer**.
- **Important:** Programmatic `.focus()` does **not** open the composer, and `type="password"` or non-text inputs do not trigger it. Read committed text from standard `onChange` / `onInput` events. Never build a custom on-screen virtual keyboard.

---

## 5. Hardware Inputs, Sensors & Software Macros (`ai-glasses-webapp-device`)

### 5.1 Discrete Pinch & D-Pad Navigation
- **D-Pad (Temple Swipes / EMG Side-Swipes):** Emits `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`. Moves focus between interactive targets.
- **EMG Index Pinch (Thumb-to-Index Pad):** Emits `Enter` and synthesizes a `click` on `document.activeElement`.
- **Single-Handler Rule:** On a focused Toolkit `<Button>`, Pinch and `Enter` already fire `onClick`. **Supply one `onClick` handler only.** Never attach a second `onKeyDown` / `onActivate` handler that synthesizes a second click, and never add time-based click debouncing (which causes dropped or stale state transitions).
- **No Pointer Lock:** `requestPointerLock()` fails with `UnknownError` on the glasses WebView and is strictly forbidden by the test gate.

### 5.2 Continuous Drag Gestures (Opt-In Only)
By default, there is no free-roaming cursor. To receive a continuous relative drag stream (for sliders, maps, drawing, or aim/move games):
1. Set `body { touch-action: none; }` **in the initial CSS stylesheet** (the glasses WebView reads `touch-action` on `<body>` once at page load; dynamic JS changes or per-element rules are ignored).
2. Listen to `pointerdown`, `pointermove` (reading `movementX` / `movementY` or client-coordinate deltas with `setPointerCapture`), and `pointerup` / `pointercancel`.
3. Clean up drag state on `pointercancel`, `visibilitychange`, route exit, and unmount.
4. **Never** set `touch-action: none` in apps that only need discrete D-pad + tap navigation.

### 5.3 IMU Sensors (Motion, Orientation, Compass, Step Detection) & Geolocation
- **Explicit User Gesture Required:** Request sensor permissions inside an explicit Start/Enable button click. Await `DeviceMotionEvent.requestPermission()` and `DeviceOrientationEvent.requestPermission()` where present.
- **APIs:**
  - `devicemotion`: `acceleration`, `accelerationIncludingGravity`, `rotationRate` (interval ~33 ms).
  - `deviceorientation` / `deviceorientationabsolute`: `alpha` (compass heading 0–360°), `beta` (pitch), `gamma` (roll).
  - `navigator.geolocation.watchPosition()`: `latitude`, `longitude`, `accuracy`, `speed`.
- **Lifecycle & Throttling:** Keep stable listener references and watch IDs. Stop them **once** on Pause/Stop, `visibilitychange` (`document.hidden`), route exit, and unmount (enforced by static analysis in `check-webapp.mjs`). Throttle React state updates to **10–30 Hz**.
- **Step Counting:** Filter acceleration magnitude $\sqrt{x^2 + y^2 + z^2}$, apply peak detection with a minimum refractory interval to reject implausibly rapid peaks, and label steps as *estimated*.
- **Deterministic Mocks & Demo Mode:** Always support deterministic sensor injection (used by `check-webapp.mjs` via `window.__WEARABLES_SENSOR_MOCK__`) and provide a clearly labeled **Demo Mode** button for desktop preview that never claims demo values are live sensor readings.

```tsx
import { useEffect, useRef, useState } from 'react';

declare global {
  interface Window {
    __WEARABLES_SENSOR_MOCK__?: {
      emitMotion: (detail?: Partial<DeviceMotionEvent>) => void;
      emitOrientation: (detail?: Partial<DeviceOrientationEvent>) => void;
    };
  }
}

export function useGlassesSensors() {
  const [status, setStatus] = useState<'idle' | 'active' | 'paused' | 'denied' | 'unsupported' | 'demo'>('idle');
  const [heading, setHeading] = useState<number | null>(null);
  const [steps, setSteps] = useState(0);
  const lastCommitRef = useRef(0);
  const lastStepRef = useRef(0);
  const watchIdRef = useRef<number | null>(null);

  const onOrientation = useRef((e: DeviceOrientationEvent) => {
    const now = performance.now();
    if (now - lastCommitRef.current < 50) return; // Throttle UI commits to 20 Hz
    lastCommitRef.current = now;
    if (typeof e.alpha === 'number') setHeading(Math.round(e.alpha));
  }).current;

  const onMotion = useRef((e: DeviceMotionEvent) => {
    const acc = e.accelerationIncludingGravity;
    if (!acc || acc.x == null || acc.y == null || acc.z == null) return;
    const mag = Math.sqrt(acc.x ** 2 + acc.y ** 2 + acc.z ** 2);
    const now = performance.now();
    // Simple peak detector with 350ms refractory period
    if (mag > 13.5 && now - lastStepRef.current > 350) {
      lastStepRef.current = now;
      setSteps(s => s + 1);
    }
  }).current;

  const stopListeners = () => {
    window.removeEventListener('deviceorientation', onOrientation);
    window.removeEventListener('devicemotion', onMotion);
    if (watchIdRef.current !== null && navigator.geolocation) {
      navigator.geolocation.clearWatch(watchIdRef.current);
      watchIdRef.current = null;
    }
  };

  const startLive = async () => {
    if (typeof DeviceMotionEvent === 'undefined' && typeof DeviceOrientationEvent === 'undefined') {
      setStatus('unsupported');
      return;
    }
    try {
      const reqMotion = (DeviceMotionEvent as unknown as { requestPermission?: () => Promise<string> }).requestPermission;
      if (typeof reqMotion === 'function') {
        const res = await reqMotion();
        if (res !== 'granted') return setStatus('denied');
      }
      const reqOrient = (DeviceOrientationEvent as unknown as { requestPermission?: () => Promise<string> }).requestPermission;
      if (typeof reqOrient === 'function') {
        const res = await reqOrient();
        if (res !== 'granted') return setStatus('denied');
      }
      window.addEventListener('deviceorientation', onOrientation);
      window.addEventListener('devicemotion', onMotion);
      if (navigator.geolocation) {
        watchIdRef.current = navigator.geolocation.watchPosition(() => {}, () => {});
      }
      setStatus('active');
    } catch {
      setStatus('denied');
    }
  };

  useEffect(() => {
    const onVisChange = () => {
      if (document.hidden && status === 'active') {
        stopListeners();
        setStatus('paused');
      }
    };
    document.addEventListener('visibilitychange', onVisChange);
    return () => {
      document.removeEventListener('visibilitychange', onVisChange);
      stopListeners();
    };
  }, [status]);

  return { status, heading, steps, startLive, pause: () => { stopListeners(); setStatus('paused'); } };
}
```

---

## 6. Real-Time 2D & 3D Game Architecture (`meta-wearables-webapp-game`)

When building a game with a real-time loop, physics, sprites, or 3D meshes, use `meta-wearables-webapp-game` instead of React.

### 6.1 Scaffolding & Updating a Game
```bash
# Scaffold a new Vite + TypeScript + Three.js + Vitest + i18next game (and run initial gate):
node plugins/meta-wearables-webapp-game/skills/create-webapp-game/scripts/init-game.mjs \
  --dir ~/webapp-games/neon-runner \
  --name neon-runner \
  --title "Neon Runner" \
  --description "Fast-paced 3D wireframe obstacle runner for Meta Ray-Ban Display."

# Re-sync managed src/framework/ code in an existing game when the plugin updates:
# (Overwrites src/framework/ only, re-stamps VERSION and <meta name="generator">)
node plugins/meta-wearables-webapp-game/skills/create-webapp-game/scripts/stamp-version.mjs ~/webapp-games/neon-runner
```

### 6.2 Strict Layer Boundaries (Enforced by `npm run validate`)
`src/framework/` is **managed engine code**—never hand-edit it. Game code lives outside `src/framework/` and must obey strict dependency boundaries so gameplay can be unit-tested in plain Node with `FakeRenderer`, `FakeInput`, `FakeAudioPlayer`, and `MemoryKeyValueStore` (`@/framework/testing/fakes` and `@/framework/storage/KeyValueStore`):

| Concern | Allowed Game Files (`src/framework/` is exempt) | Rule |
|---|---|---|
| `three` imports | **`src/models.ts` ONLY** | Gameplay (`src/core/`) never imports `three`; it uses `Renderer<ModelId>` and `Vector3` from `@/framework/math/Vector3`. |
| DOM (`document`, `window`, `localStorage`) | **`src/main.ts`, `src/hud/`, `src/log.ts` ONLY** | Gameplay never touches DOM or `localStorage`. HUD reads game state and writes DOM. Persistence uses injected `KeyValueStore`. |
| DOM Input Events (`click`, `keydown`, `pointer*`) | **NONE in game code** (`PointerKeyboardInput.ts` only) | Never put `onclick`, `addEventListener('keydown')`, etc. in HTML or game/HUD code. All input flows through `InputManager`. |
| Web Audio API & Concrete Audio Backend | **NONE in gameplay** (`src/main.ts` constructs `AmpAudioPlayer`) | Gameplay takes injected `AudioPlayer<SoundId>`. Never call `new Audio()` or `AudioContext` in game code. |
| Logging (`console.*`) | **NONE in game code** | Use `import { log } from '@/log'` (`log.info`, `log.warn`, `log.error`, `log.child('scope')`). Guard hot loops with `if (log.isEnabled('debug'))`. |
| User-Facing Text | **`src/i18n/en.json` ONLY** | Render dynamic text via `t('key')` (`@/i18n`) and static HTML via `data-i18n="key"`. Never assign raw letter strings to `.textContent`. |
| Tunable Numbers | **`src/config/gameplayConstants.ts` ONLY** | Never inline magic numbers in gameplay. Do **not** put `as const` on tunable blocks (only `DISPLAY` uses `as const`). |

### 6.3 The Renderer Contract & Visual Juice Channels
- **Three.js for the World, DOM for Text:** Never render text in WebGL. Use `#hud` (DOM overlay) for all scores, menus, and text.
- **2D vs. 3D Camera:** `new ThreeRenderer(canvas, MODELS)` defaults to `'perspective'` (3D). Pass `{ projection: 'orthographic', viewSize: 5 }` for flat 2D games.
- **Per-Instance Visual Channels:** `Renderer<ModelId>` provides `addModel`, `setTransform`, `setRotation`, `setVisible`, `remove`, plus three per-instance feel channels (isolated per instance because `ThreeRenderer` clones materials in `addModel`):
  - `renderer.setScale?.(handle, scale: number | Vector3)` — pulse, pop, or squash-and-stretch.
  - `renderer.setOpacity?.(handle, opacity: number)` — fade in/out, damage flash, ghosting (`0..1` clamped; throws on `NaN`).
  - `renderer.setFrame?.(handle, frameIndex: number)` — 2D spritesheet animation (declare `sheet` and `frames: AtlasFrame[]` on the `ModelSpec` in `src/models.ts`).

### 6.4 Art, Spritesheets & Preload-Only Asset Loading
1. **Additive Display Art Rule:** Any near-black sprite (`#000000` to `#141414`) emits no light and is **invisible on the glasses**. Hazards and dodgeable objects must be among the brightest elements drawn. Never use an opaque full-bleed background tile (it floods the wearer's real-world view).
2. **Spritesheet Windowing & Silhouette Sizing (`docs/spritesheets.md`):**
   - Never pass a shared spritesheet directly to `createTexturedPlane` and mutate `.offset` (UV `offset`/`repeat` live on `THREE.Texture`, collapsing all sprites to the last frame set).
   - Always cut frames with `atlasFrameTexture(sheet, { x, y, w, h })` or `atlasSprite(sheet, { x, y, w, h }, { width, height })`, which clone the `THREE.Texture` while sharing one GPU `Source`.
   - Measure the non-transparent silhouette inside padded cells (`magick sheet.png -crop 128x128+0+0 +repage -trim info:-`) and scale the quad by `intendedSize / (contentSize / cellSize)`.
3. **3D Models (`docs/asset-loading.md`):**
   - Clone rigged/skinned models with `cloneModel(obj)` (`SkeletonUtils.clone`), never `obj.clone()`.
   - Convert lit materials (`MeshStandardMaterial`) to unlit `MeshBasicMaterial` (preserving `.map` and `.vertexColors`) since the additive scene has no lights by default.
4. **Manifest Preloading & Runtime Seals (`docs/loading-screen.md`):**
   - Declare all assets (`texture`, `model`, `modelWithAnimations`, `audio`, `raw`) in an `as const satisfies Record<string, AssetManifestEntry>` manifest.
   - Load them before the title screen using `preloadManifest` + `LoadingScreen`, then call `sealAssetLoaders()` and `sealAssetNetwork({ strict: strictSealRequested(search) })` so any accidental mid-game network fetch fails loudly.

### 6.5 Game Input Modes: Tap-Only vs. Opt-In Drag (`docs/drag-channel.md`)
- **Tap-Only (Default):** `PointerKeyboardInput()` with no options. D-pad swipes → `dpadSwipe` (`'up'|'down'|'left'|'right'`); Index pinch (`Enter` on desktop) → `pinchTap`. Mouse clicks do nothing.
- **Drag Mode (3 Coordinated Edits):** Only enable if a drag genuinely drives gameplay (never just to make desktop mouse clicks work—`validate-drag-optin.mjs` fails if enabled without consuming the delta):
  1. `src/style.css`: Add `touch-action: none;` to `html, body`.
  2. `src/config/gameplayConstants.ts`: Add `export const INPUT = { mouseSensitivity: 0.01, deviceSensitivity: 0.02, tapMaxTravelPx: 6 };`.
  3. `src/main.ts` & `src/core/Game.ts`: Pass `{ sensitivity, tapMaxTravelPx, pointerDrag: true }` to `new PointerKeyboardInput(...)` and call `this.input.consumeMovementDelta()` in `update(dt)`. Apply the returned delta directly as a **displacement** (do **not** multiply by `dt`). In drag mode, a zero-travel click fires `pinchTap` and `Enter` is ignored.

### 6.6 Declarative Audio Engine & Memory Banks (`docs/audio.md`, `docs/audio-banks.md`)
- **Declarative Catalog (`src/audio/audioSettings.json`):** Designer-owned JSON defining `global`, `banks`, `voiceGroups`, and `events` (each with `clips` of sample filenames or inline `synth` oscillator/noise stacks, `volume`, `pitch`, `clipSelect`, `cooldown`, `voiceLimit`, `voiceLimitStrategy`, `bus: 'sfx' | 'music'`).
- **Compile-Checked IDs (`src/audio/soundIds.ts`):** Export `SOUND_IDS = [...] as const` and `export type SoundId = (typeof SOUND_IDS)[number]`. `audioSettings.test.ts` ensures the JSON and union never drift.
- **Bank Memory Management:** Decoded 48 kHz stereo PCM costs **~0.37 MB/sec** (~30× Ogg Vorbis size). Preload sample clips with `{ type: 'audio', path: '...', bank: 'level_1' }` (retains compressed `Blob` in memory, zero runtime network I/O) and run `npm run audio-sizes` to generate `src/audio/audioSizes.json` (`withAudioSizes(MANIFEST, SIZES)`). Decode/free per level via `await audio.loadBank('level_1')`, `audio.unloadBank('level_1')`, or `await audio.swapBanks(['shared'], ['level_2'])` within the 24 MB `AUDIO.maxResidentBytes` ceiling. Music bus events stream from `blob:` URLs by default (`stream: true`) to cost 0 PCM budget.

---

## 7. Performance Optimization, Testing & Debugging

### 7.1 Deterministic WebApp Quality Gate (`ai-glasses-webapp-test`)
Run after every React/UI Toolkit app implementation:
```bash
node plugins/meta-wearables-webapp/skills/ai-glasses-webapp-test/scripts/check-webapp.mjs <app-dir>
```
What it enforces:
- Verifies `package-lock.json`, `@wearables-ui-toolkit/mrbd` and `@wearables-ui-toolkit/icons` registry versions, `mrbd-web-app-capable="yes"`, non-placeholder title/description, cleanup of `setInterval` / `watchPosition` / sensor listeners, and zero findings from `validate-ui-toolkit.mjs`.
- Runs `npm run typecheck` and `npm run build`, verifying gzipped JS `< 300 KB`.
- Launches headless Playwright (`600×600` device viewport and `1000×800` desktop viewport), injects `window.__WEARABLES_SENSOR_MOCK__`, verifies edge-to-edge root sizing, zero document overflow, accessible names on all focusables, arrow-key focus movement, `Enter` activation, `Escape` focus retention, `< 15` requests, `< 128 MB` heap, `>= 27 fps`, and writes `normal.png` + `additive-composite.png` to `.wearables-test/`.
- **Local Preview Contract:** When finishing a new or redesigned app build, start a persistent preview server (`npm run preview -- --host 127.0.0.1 --port 4173`) and give the user the clickable `http://127.0.0.1:4173/` URL.

### 7.2 CDP Throttled Performance Profiling (`ai-glasses-webapp-optimize-performance`)
Before release, measure production builds over the Chrome DevTools Protocol under the glasses hardware profile (`500 Kbps down`, `150 ms RTT`, `12× CPU throttling`, `600×600 DPR 1`):

```bash
# 1. Build and serve production output:
npm run build && npm run preview -- --host 127.0.0.1 --port 5173 --strictPort

# 2. Launch Chrome with a throwaway profile:
PERF_PROFILE="$(mktemp -d "${TMPDIR:-/tmp}/ai-glasses-perf.XXXXXX")"
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --remote-debugging-address=127.0.0.1 --remote-debugging-port=9222 --user-data-dir="$PERF_PROFILE"

# 3. Measure Cold & Warm Startup (n >= 10, overlay-aware content oracle required):
SKILLS=plugins/meta-wearables-webapp/skills/ai-glasses-webapp-optimize-performance/scripts
node $SKILLS/measure.mjs --url http://127.0.0.1:5173 --reps 10 --clear-storage --by-type \
  --oracle "(() => { const t = document.body.innerText || ''; const s = document.querySelector('.splash'); return /Forecast/.test(t) && (!s || getComputedStyle(s).opacity === '0'); })()"
node $SKILLS/measure.mjs --url http://127.0.0.1:5173 --reps 10 --clear-storage --warm \
  --oracle "(() => { const t = document.body.innerText || ''; return /Forecast/.test(t); })()"

# 4. Extract app-owned Critical CSS & Audit Oversized Images:
node $SKILLS/critical-css.mjs --url http://127.0.0.1:5173 --clear-storage --oracle "..."
node $SKILLS/audit-images.mjs --url http://127.0.0.1:5173 --clear-storage --oracle "..."

# 5. Interleaved A/B Comparison + Null Control (noise floor check):
node $SKILLS/ab.mjs --a ./dist-before --b ./dist-after --reps 10 --clear-storage --oracle "..."
node $SKILLS/ab.mjs --a ./dist-before --null --reps 10 --clear-storage --oracle "..."
```

### 7.3 Game Validation, Browser Iteration & On-Glasses Logging
For games built with `meta-wearables-webapp-game`:
- **Static & Unit Gate:**
  ```bash
  npm run typecheck && npm test && npm run build && npm run validate
  # Always report game tests separately from inherited framework tests:
  npm run test:game
  npm run test:framework
  ```
- **Deterministic Browser Verification (`iterate-webapp-game`):**
  Wire `npm run chrome` via `setup-cdp.mjs`, start Vite (`npm run dev -- --host 127.0.0.1 --strictPort < /dev/null`), open with `?drive` (`cdp.mjs open --url "http://127.0.0.1:5173/?drive"`), step frames deterministically (`cdp.mjs step --frames 30`), inspect `window.__game`, capture stage screenshots (`cdp.mjs shot --selector "#game-root" --out /tmp/frame.png`), and verify additive-display luminance numerically with `frame-stats.mjs --png /tmp/frame.png` (checking `nonBlackBBox`, `meanLuminance`, `fractionNearBlack`, and `fractionLit <= 0.15` on sky/backdrop bands).
- **URL Debug Switches (`docs/query-parameters.md`):**
  - `?stats` — Live FPS / CPU ms / draw calls / triangles / GPU & audio PCM memory overlay.
  - `?log=debug&logview` — Draws the last 10 log lines directly onto the 600×600 display (no backend required).
  - `?drive` — Pauses `GameLoop` for external stepping via `window.__webappGame.step(n)`.
  - `?slowload=1500` — Staggers `preloadManifest` to preview `LoadingScreen` on device.
  - `?strict` — Makes `sealAssetNetwork` throw instead of warn on stray runtime network requests.
  - `?swreset=1` — Unregisters the offline precache Service Worker and clears all `CacheStorage` on the glasses.
  - `?mute` — Starts session muted.
  - `?lng=<code>` — Overrides locale detection for testing translations.
- **Remote Device Logging (`/add-webapp-game-logging`):**
  Installs Vercel serverless endpoints (`/api/logs`, `/api/log-auth`) and a passcode-gated `/logs` web portal (`LOG_TOKEN`). Enforces a blocking on-glasses `ConsentGate` before any record leaves the device, redacts `LOG_TOKEN`, and generates double-percent-encoded `fb-viewapp://` debug deep links via `node scripts/debug-url.mjs`.

---

## 8. Publishing & Deployment (`ai-glasses-webapp-publish`)

Only publish when the user explicitly asks to deploy publicly.

### 8.1 Standard React WebApps
Run the automated Vercel production publisher (requires authenticated `vercel` CLI and `python3`):
```bash
node plugins/meta-wearables-webapp/skills/ai-glasses-webapp-publish/scripts/publish-to-vercel.mjs <app-dir>
```
This script runs `check-webapp.mjs`, verifies `vercel whoami`, deploys via `vercel --prod --yes`, confirms the HTTPS URL is anonymously accessible (not behind Vercel Deployment Protection), and generates `qr-publish.png` encoding the Meta AI deep link:
```text
fb-viewapp://web_app_deep_link?appName=<url-encoded-name>&appUrl=<url-encoded-https-url>
```

### 8.2 WebApp Games (Vercel & Single-File Packaging)
- **Do NOT run `publish-to-vercel.mjs` on a game** (it runs the React UI Toolkit test gate).
- **Deploy from the project root** with `vercel --prod` (never add a `server.js` or `package.json` `start` script, and never move `vercel.json` into `dist/`).
- Generate the add-to-glasses QR code using `qr_generator.py`:
  ```bash
  python3 plugins/meta-wearables-webapp/skills/ai-glasses-webapp-publish/scripts/qr_generator.py \
    --png qr-publish.png "fb-viewapp://web_app_deep_link?appName=Neon%20Runner&appUrl=https%3A%2F%2F..."
  ```
- **Single-File Artifact (`npm run ship`):** For procedural/data-URI games, `npm run ship` runs all gates, builds, and executes `scripts/package-single-file.mjs` to emit `dist/index.single.html` and `dist/prototype-artifact.json` (inlining all JS/CSS/icons with a literal boot shell).
```
