---
name: gemini-interactions-api
description: >
  Provides coding patterns for the @google/genai TypeScript SDK. Covers using the
  Interactions API for the Antigravity agent, Deep Research agent, Omni models (Omni Flash),
  and general model capabilities (text, image, speech generation), as well as other specialized
  APIs for music (Lyria) and real-time audio (Live API) in a server-side context.
  Note that Omni models and Agents are only available via the Interactions API, not with
  standard generateContent.
  Use when user requests the Interactions API, Antigravity agent, Deep Research agent, or Omni models.
---

# Gemini Interactions API

## Prerequisites

-   **JavaScript/TypeScript**: `@google/genai` >= `2.4.0` → `npm install
    @google/genai`

> [!IMPORTANT]
>
> The Interactions API requires `@google/genai` version `2.4.0` or later. If
> your `package.json` has a `1.x.x` version, you must explicitly upgrade by
> running `npm i @google/genai@latest` or `npm i @google/genai@^2.4.0`. A simple
> `npm update` will not cross the major version boundary.

## @google/genai Models

> [!IMPORTANT]
>
> The models listed in this section are the absolute source of truth when
> selecting a model. Even if a specific use case is not listed in the examples
> or sections below, you must still choose one of the models defined here,
> unless the user specifies one.

> [!CAUTION]
>
> **NEVER** use the following deprecated models. They are strictly prohibited
> and unsupported: - `gemini-1.5-flash` - `gemini-1.5-pro` - `gemini-pro`
>
> Always select a valid model from the list below.

-   If the user provides a full model name that includes hyphens, a version, and
    an optional date (e.g., gemini-2.5-flash-preview-12-2025 or
    gemini-3.1-pro-preview), use it directly.
-   If the user provides a common name or alias, use the following full model
    name.
    -   gemini flash: 'gemini-flash-latest'
    -   gemini lite or flash lite: 'gemini-3.1-flash-lite'
    -   gemini pro: 'gemini-3.1-pro-preview'
    -   nano banana, or gemini flash image, or nano banana lite:
        'gemini-3.1-flash-lite-image'
    -   nano banana 2: 'gemini-3.1-flash-image'
    -   nano banana pro, or gemini pro image: 'gemini-3-pro-image'
    -   native audio or gemini flash audio: 'gemini-3.1-flash-live-preview'
    -   live translation: 'gemini-3.5-live-translate-preview'
    -   gemini tts or gemini text-to-speech: 'gemini-3.1-flash-tts-preview'
    -   gemini omni flash: 'gemini-omni-flash-preview'
    -   Lyria Clip: 'lyria-3-clip-preview'
    -   Lyria Pro: 'lyria-3-pro-preview'
-   If the user does not specify any model, select the following model based on
    the task type.
    -   Basic Text Tasks (e.g., summarization, proofreading, and simple Q&A):
        'gemini-3.5-flash'
    -   Complex Text Tasks (e.g., advanced reasoning, coding, math, and STEM):
        'gemini-3.1-pro-preview'
    -   General Image Generation and Editing Tasks:
        'gemini-3.1-flash-lite-image'
    -   High-Quality Image Generation and Editing Tasks (supports 512px, 1K, 2K,
        and 4K resolution): 'gemini-3.1-flash-image'
    -   Video Generation and Editing Tasks: 'gemini-omni-flash-preview'
    -   Real-time audio & video conversation tasks:
        'gemini-3.1-flash-live-preview'
    -   Real-time speech translation tasks: 'gemini-3.5-live-translate-preview'
    -   Text-to-speech tasks: 'gemini-3.1-flash-tts-preview'
    -   Short music clip generation tasks (30s): 'lyria-3-clip-preview'
    -   Full-length music generation tasks: 'lyria-3-pro-preview'
    -   Embedding tasks: 'gemini-embedding-2-preview'

The following models require a paid API key. Prompt the user to select one by
calling the `show_aistudio_ui` tool with arguments: `ui:type: "paid_model_flow"`
*before* using these models.

-   `gemini-3.1-pro-preview`
-   `gemini-3-pro-image`
-   `gemini-3.1-flash-image`
-   `gemini-3.1-flash-lite-image`
-   `gemini-omni-flash-preview`
-   `lyria-3-clip-preview`
-   `lyria-3-pro-preview`

## @google/genai Agents

The following agents are available for use with the Interactions API:

-   `antigravity-preview-05-2026`: Antigravity Agent — general-purpose managed
    agent with code execution, file management, and web access in a sandboxed
    Linux environment.
-   `deep-research-preview-04-2026`: Deep Research — fast, interactive
-   `deep-research-max-preview-04-2026`: Deep Research Max — maximum
    exhaustiveness
-   **Custom agents**: Create your own via `ai.agents.create()`.

All agents require a paid API key. Prompt the user to select one by calling the
`show_aistudio_ui` tool with arguments: `ui:type: "paid_model_flow"` *before*
using any agent.

## Important Additional Notes

-   Interactions are **stored by default** (`store=true`). Paid tier retains for
    55 days, free tier for 1 day.
-   Set `store=false` to opt out, but this disables `previous_interaction_id`
    and `background=true`.
-   `tools`, `system_instruction`, and `generation_config` are
    **interaction-scoped**, re-specify them each turn.
-   **Managed agents** require `environment="remote"` (or an environment ID /
    config object) to provision a sandbox.
-   **Migrating from `generateContent`**: Always confirm scope with the user
    before editing.

## Agent-Specific Guidance

For detailed concepts, capabilities, and code patterns for each agent, read the
corresponding reference file:

-   `references/managed-agents.md` — Managed Agents overview (polling,
    multi-turn, streaming, files)
-   `references/antigravity-agent.md` — Antigravity Agent (default agent, tools,
    multimodal input)
-   `references/custom-agents.md` — Custom Agents (creation, sources, network
    rules, custom instructions)
-   `references/deep-research.md` — Deep Research Agent (collaborative planning,
    visualization, streaming, MCP, file search)
-   `references/agent-environment.md` — Agent Environments (sandbox config,
    custom sources, network allowlist, snapshots)

## Gemini Interactions API guidance

## @google/genai Coding Guidelines

Use the `@google/genai` SDK to call Gemini agents.

### Calling Gemini Interactions API

-   **Always** call Gemini Interactions API from the server-side code of the
    application.
-   **NEVER** call Gemini Interactions API directly from the client/browser
    code.
-   **NEVER** expose the API key to the browser.
-   **No Static Explanatory UI or Chat Explanations**: Do **not** explain how
    agents (like Antigravity or Deep Research) work in your chat responses, and
    do **not** generate static informational UI components (such as "About"
    cards or help paragraphs) explaining the sandbox container infrastructure in
    the code, unless explicitly requested. However, you **should** build dynamic
    UI elements (often referred to as showing "proof of work") to display the
    agent's live progress, sub-steps, reasoning thoughts, and tool calls from
    the `interaction.steps` response.
-   **Agents and Omni models require Interactions API**: Managed agents (such as
    `antigravity-preview-05-2026` or `deep-research-preview-04-2026`), custom
    agents, and Omni models (like `gemini-omni-flash-preview`) can **ONLY** be
    invoked using the `ai.interactions.create()` method. They **cannot** be
    invoked using `ai.models.generateContent()` or
    `ai.models.generateContentStream()`. Calling a model using `generateContent`
    is only for standard models (like `gemini-3.5-flash`), not agents or Omni
    models.

The server handles all `@google/genai` SDK calls. The client communicates with
the server through your application's API endpoints (e.g., Next.js API routes,
Express routes, or Angular SSR endpoints).

If a server-side Interactions API call fails or errors, propagate the error back
to the client (via the HTTP response, SSE stream, or WebSocket) so the frontend
can display it.

### Server-Side Initialization

Create a shared Gemini client utility on the server:

```ts
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({ apiKey: process.env.GEMINI_API_KEY });
```

### Client-Side Usage

The client never imports `@google/genai`. Instead, it calls your server's API
routes, for example:

```ts
const response = await fetch("/api/research", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ prompt: "Research the history of Google TPUs." }),
});
const { interactionId } = await response.json();
```

### Incorrect Usages

Do *not* use or import the following types from `@google/genai`; these are
deprecated APIs and no longer work.

-   **Incorrect** `GoogleGenerativeAI`
-   **Incorrect** `google.generativeai`
-   **Incorrect** `models.create`
-   **Incorrect** `ai.models.create`
-   **Incorrect** `models.getGenerativeModel`
-   **Incorrect** `genAI.getGenerativeModel`
-   **Incorrect** `ai.models.getModel`
-   **Incorrect** `ai.models['model_name']`
-   **Incorrect** `generationConfig`
-   **Incorrect** `GoogleGenAIError`
-   **Incorrect** `GenerateContentResult`; **Correct**
    `GenerateContentResponse`.
-   **Incorrect** `GenerateContentRequest`; **Correct**
    `GenerateContentParameters`.
-   **Incorrect** `SchemaType`; **Correct** `Type`.

Do *not* import `@google/genai` or instantiate `GoogleGenAI` in any client-side
/ browser code unless the user explicitly asks for it, when they do make sure to
highlight the security implications of doing so. All Gemini SDK usage must be
server-side for as a default.

### API Key

-   **Server-only:** The API key is accessed via `process.env.GEMINI_API_KEY` on
    the server. It must **never** be sent to the browser or included in
    client-side bundles.

-   **Incorrect Initialization:** `const ai = new GoogleGenAI(apiKey);` // Must
    use a named parameter `{ apiKey: ... }`.

-   **No UI for API Key:** Do **not** generate any UI elements (input fields,
    forms, prompts, configuration sections) or code snippets for entering or
    managing the API key. Do **not** request that the user update the API key in
    the code. The key's availability is handled externally and is a hard
    requirement. The application **must not** ask the user for it under any
    circumstances.

### Import

-   Always use `import {GoogleGenAI} from "@google/genai";`
-   **Prohibited:** `import { GoogleGenerativeAI } from "@google/genai";`
-   **Prohibited:** `import type { GoogleGenAI} from "@google/genai";`
-   **Prohibited:** `declare var GoogleGenAI`.
-   **Prohibited:** Any `@google/genai` import in client-side code.

## Data Model

An `Interaction` response contains `steps`, an array of typed step objects
representing a structured timeline of the interaction turn.

### Step Types

**User steps:**

-   `user_input`: User input (text, audio, multimodal). Contains `content`
    array.

**Model/server steps:**

-   `model_output`: Final model generation. Contains `content` array with
    `text`, `image`, `audio`, etc.
-   `thought`: Model reasoning/Chain of Thought. Has `signature` field
    (required) and optional `summary`.
-   `function_call`: Tool call request (`id`, `name`, `arguments`).
-   `function_result`: Tool result you send back (`call_id`, `name`, `result`).
-   `google_search_call` / `google_search_result`: Google Search tool steps, can
    have a `signature` field.
-   `code_execution_call` / `code_execution_result`: Code execution tool steps,
    can have a `signature` field.
-   `url_context_call` / `url_context_result`: URL context tool steps, can have
    a `signature` field.
-   `mcp_server_tool_call` / `mcp_server_tool_result`: Remote MCP tool steps.
-   `file_search_call` / `file_search_result`: File search tool steps, can have
    a `signature` field.

### Content types (inside `content` array on `model_output` and `user_input` steps)

-   `text`: Text content (`text` field)
-   `image` / `audio` / `document` / `video`: Content with `data`, `mime_type`,
    or `uri`

### Response Helpers

The SDK provides convenience properties on the `Interaction` response object:

-   `output_text` (`string | null`): The last consecutive run of text from the
    trailing `model_output` steps.
-   `output_image` (`Image | null`): The last image generated by the model.
    Returns an object with `data` (base64) and `mime_type`.
-   `output_audio` (`Audio | null`): The last audio generated by the model.
    Returns an object with `data` (base64) and `mime_type`.
-   `output_video` (`Video | null`): The last video generated by the model.
    Returns an object with `data` (base64) or `uri`, and `mime_type`.

> [!WARNING]
>
> During long-running agent execution (such as Antigravity, custom agents, or
> Deep Research), the agent generates output incrementally across multiple
> chronological `model_output` steps (e.g., intermediate drafts, code execution
> outputs, and sections) as it runs, rather than writing everything in a single
> step. Because `output_text` only returns the text from the **very last
> consecutive step**, using `interaction.output_text` on an agent result may cut
> off the response, showing only the concluding section and losing the earlier
> sections. For all agent interactions, you should iterate and combine the text
> parts from all `model_output` steps in the `steps` array:
>
> ```typescript
> let fullOutput = "";
> for (const step of interaction.steps) {
>   if (step.type === 'model_output') {
>     const textContent = step.content?.find(c => c.type === 'text');
>     if (textContent && textContent.text) {
>       fullOutput += textContent.text;
>     }
>   }
> }
> ```

### Safe JSON Extraction from Agent Output

> [!NOTE]
>
> Standard model calls support structured outputs natively (using
> `response_format`), but **managed agents (like Antigravity and Deep Research)
> do not support structured outputs**. If you need structured JSON from an
> agent, you must ask for it in the text prompt (instructing the agent to write
> JSON). Because this relies on text instructions, the agent may wrap it in
> markdown code blocks or append conversational text. Use the following regex
> matching pattern to safely extract and parse the JSON from the combined steps
> output:

````typescript
let parsedReport = null;
const jsonMatch = fullOutput.match(/```json\s*([\s\S]*?)\s*```/) || fullOutput.match(/([\{\[][\s\S]*[\}\]])/);
if (jsonMatch) {
  try {
    parsedReport = JSON.parse(jsonMatch[1]);
  } catch (err) {
    // Implement lenient cleanup logic to catch trailing commas or shell outputs inside the code blocks
  }
}
````

## General Interactions Examples

Create a simple model interaction with a text input.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "why is the sky blue?",
});
console.log(interaction.output_text);
```

### Extracting Text Output from Interaction Response

The Interaction response contains a `steps` array. You can iterate through the
`steps` to find the model's output.

```ts
for (const step of interaction.steps) {
  if (step.type === 'model_output') {
    const textContent = step.content?.find(c => c.type === 'text');
    if (textContent) {
      console.log(textContent.text);
    }
  }
}
```

### System Instruction and Other Model Configs

Create an interaction with a system instruction and other model configs.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "Tell me a story.",
  system_instruction: "You are a storyteller for kids under 5 years old.",
  generation_config: {
    top_p: 0.95,
    temperature: 1,
    seed: 42,
    max_output_tokens: 1024,
  },
});
```

### Thinking Config

The Thinking Config is only available for the Gemini 3 and 2.5 series models. Do
not use it with other models. For Game AI Opponents / Low Latency: *Disable*
thinking by setting `thinking_level` to `"minimal"` in the `generation_config`:

```ts
generation_config: {
  thinking_level: "minimal",
}
```

For All Other Tasks: *Omit* `thinking_level` entirely.

### JSON Response

Ask the model to return a response in JSON format.

```ts
import { GoogleGenAI, Type } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "List a few popular cookie recipes, and include the amounts of ingredients.",
  response_format: {
    type: Type.ARRAY,
    items: {
      type: Type.OBJECT,
      properties: {
        recipeName: {
          type: Type.STRING,
          description: "The name of the recipe.",
        },
        ingredients: {
          type: Type.ARRAY,
          items: {
            type: Type.STRING,
          },
          description: "The ingredients for the recipe.",
        },
      },
    },
  },
});
const lastStep = interaction.steps.at(-1);
let jsonStr = '';
if (lastStep.type === 'model_output') {
  const textContent = lastStep.content?.find(c => c.type === 'text');
  if (textContent) {
    jsonStr = textContent.text.trim();
  }
}
```

### Function calling

To let Gemini interact with external systems, you can provide function tool
declarations.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: "gemini-3.5-flash",
  tools: [{
    type: "function",
    name: "controlLight",
    description: "Set the brightness and color temperature of a room light.",
    parameters: {
      type: "object",
      properties: {
        brightness: {
          type: "number",
          description: "Light level from 0 to 100. Zero is off and 100 is full brightness.",
        },
        colorTemperature: {
          type: "string",
          description: "Color temperature of the light fixture such as `daylight`, `cool` or `warm`.",
        },
      },
      required: ["brightness", "colorTemperature"],
    },
  }],
  input: "Dim the lights so the room feels cozy and warm.",
});
```

### Combine Function Calling with the other built-in tools

Gemini 3 series models support combining built-in tools (e.g., `google_search`,
`url_context`) with function calling in the same request.

```ts
const interaction1 = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "What is the northernmost city in the United States? What's the weather like there today?",
  tools: [
    { type: 'google_search' },
    {
      type: "function",
      name: "getWeather",
      description: "Get the weather in a given location",
      parameters: {
        type: "object",
        properties: {
          location: {
            type: "string",
            description: "The city and state, e.g. San Francisco, CA"
          }
        },
        required: ["location"]
      }
    }
  ],
});
```

If you need to make additional calls to the model after the first request, you
can use `previous_interaction_id` to preserve the context.

```ts
const interaction2 = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "What is the weather like there tomorrow?",
  previous_interaction_id: interaction1.id,
  tools: [
    { type: 'google_search' },
    {
      type: "function",
      name: "getWeather",
      description: "Get the weather in a given location",
      parameters: {
        type: "object",
        properties: {
          location: {
            type: "string",
            description: "The city and state, e.g. San Francisco, CA"
          }
        },
        required: ["location"]
      }
    }
  ],
});
```

### Create Interaction (Streaming)

Set `stream: true` to receive incremental server-sent events. Each stream
follows: `interaction.created` → (`step.start` → `step.delta`(s) → `step.stop`)+
→ `interaction.completed`.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const stream = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "Tell me a story in 300 words.",
  stream: true,
});
for await (const event of stream) {
    if (event.event_type === "step.delta") {
        if (event.delta.type === "text") {
            process.stdout.write(event.delta.text);
        }
    } else if (event.event_type === "interaction.completed") {
        console.log(`\n\nTotal Tokens: ${event.interaction.usage.total_tokens}`);
    }
}
```

### Multi-turn Interactions

Use `previous_interaction_id` to chain interactions for multi-turn
conversations.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction2 = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "How are you?",
  previous_interaction_id: interaction1.id,
});
```

### Search Grounding

Use Google Search grounding for queries related to recent events or web
information.

```ts
const interaction = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "Who won the last Super Bowl?",
  tools: [{ type: 'google_search' }],
});
```

### URL Context

The URL context tool lets you provide additional context to the models in the
form of URLs.

```ts
const interaction = await ai.interactions.create({
  model: "gemini-3.5-flash",
  input: "Summarize the recent events based on https://www.sfmoma.org",
  tools: [{ type: 'url_context' }],
});
```

### Maps Grounding

Use Google Maps grounding for queries that relate to geography or place
information.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: 'gemini-3.5-flash',
  input: "What's the best coffee shop near me?",
  tools: [{ type: 'google_maps' }]
});
```

### Image Generation and Editing

Generate images using `gemini-3.1-flash-lite-image` by default.

#### Examples

Use `ai.interactions.create` to generate images with nano banana series models

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: 'gemini-3.1-flash-image',
  input: "A robot holding a red skateboard.",
  response_modalities: ['image', 'text'],
  generation_config: {
    image_config: {
      aspect_ratio: "1:1",
      image_size: "1K"
    },
  },
});
for (const step of interaction.steps) {
  if (step.type === 'model_output') {
    const imageContent = step.content?.find(c => c.type === 'image');
    if (imageContent && imageContent.data) {
      const base64EncodeString: string = imageContent.data;
      const mimeType = imageContent.mime_type || 'image/png';
      const imageUrl = `data:${mimeType};base64,${base64EncodeString}`;
    }
  }
}
```

To edit images:

```ts
const interaction = await ai.interactions.create({
  model: 'gemini-3.1-flash-lite-image',
  input: [
    {
      type: "image",
      data: base64ImageData,
      mime_type: "image/png",
    },
    {
      type: "text",
      text: "can you add a llama next to the image",
    },
  ],
});
```

### Speech Generation (TTS)

Transform text input into audio. Note that TTS does not support streaming.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...
const interaction = await ai.interactions.create({
  model: 'gemini-3.1-flash-tts-preview',
  input: 'Say the following: WOOHOO This is so much fun!',
  response_modalities: ['AUDIO'],
  generation_config: {
    speech_config: {
      language: "en-us",
      voice: "kore" // 'puck', 'charon', 'kore', 'fenrir', 'zephyr'
    }
  }
});

for (const step of interaction.steps) {
  if (step.type === 'model_output') {
    const audioContent = step.content?.find(c => c.type === 'audio');
    if (audioContent && audioContent.data) {
      const audioBuffer = Buffer.from(audioContent.data, 'base64');
      // ... return buffer to client or save to file ...
    }
  }
}
```
#### Multi-speakers

Use it when you need 2 speakers (the number of `speaker_voice_configs` must
equal 2).

```ts
// ... initialization ...

const prompt = `TTS the following conversation between Joe and Jane:
      Joe: How's it going today Jane?
      Jane: Not too bad, how about you?`;

const interaction = await ai.interactions.create({
  model: "gemini-3.1-flash-tts-preview",
  input: prompt,
  response_modalities: ['AUDIO'],
  generation_config: {
    speech_config: {
      multi_speaker_voice_config: {
        speaker_voice_configs: [
          {
            speaker: 'Joe',
            voice_config: {
              prebuilt_voice_config: { voice_name: 'Kore' }
            }
          },
          {
            speaker: 'Jane',
            voice_config: {
              prebuilt_voice_config: { voice_name: 'Puck' }
            }
          }
        ]
      }
    }
  }
});

for (const step of interaction.steps) {
  if (step.type === 'model_output') {
    const audioContent = step.content?.find(c => c.type === 'audio');
    if (audioContent && audioContent.data) {
      const audioBuffer = Buffer.from(audioContent.data, 'base64');
      // ... return buffer to client or save to file ...
    }
  }
}
```

## Managed Agents

Managed agents run inside a sandboxed Linux environment hosted by Google.

### Antigravity Agent

The Antigravity agent (`antigravity-preview-05-2026`) is the general-purpose
managed agent. It can execute code (Bash, Python, Node.js), manage files, browse
the web, and use Google Search.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Write a Python script that generates the first 20 Fibonacci numbers and saves them to fibonacci.txt. Then read the file and print its contents.",
    environment: "remote",
}, { timeout: 300000 });

console.log(`Environment ID: ${interaction.environment_id}`);
console.log(interaction.output_text);
```

### Custom Agents

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

const agent = await ai.agents.create({
    id: "code-reviewer",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You are a senior code reviewer. Check every file for bugs, style issues, and security vulnerabilities.",
    base_environment: {
        type: "remote",
        sources: [
            {
                type: "repository",
                source: "https://github.com/my-org/backend",
                target: "/workspace/repo",
            }
        ],
    },
});

// Invoke — each call forks the base environment
const result = await ai.interactions.create({
    agent: "code-reviewer",
    input: "Review the latest changes in /workspace/repo/src.",
    environment: "remote",
}, { timeout: 300000 });
console.log(result.output_text);
```

Manage agents with `ai.agents.list()`, `ai.agents.get(id)`, and
`ai.agents.delete(id)`.

## Deep Research

Use the Interactions API for deep research tasks. These operations are
long-running and can take several minutes to complete, so they must be run in
the background (`background: true`) and polled or streamed.

For advanced features like collaborative planning, streaming with automatic
reconnection, visualization (charts/graphs), tool configuration (MCP/File
Search), steerability, and multimodal input, read the corresponding reference
file `references/deep-research.md`.

```ts
import { GoogleGenAI } from "@google/genai";

// ... initialization ...

// Start background research
const initialInteraction = await ai.interactions.create({
    agent: "deep-research-preview-04-2026",
    input: "Research the history of Google TPUs.",
    background: true,
});

// Poll for results
while (true) {
    const interaction = await ai.interactions.get(initialInteraction.id);
    if (interaction.status === "completed") {
        let fullReport = "";
        for (const step of interaction.steps) {
            if (step.type === 'model_output') {
                const textContent = step.content?.find(c => c.type === 'text');
                if (textContent) fullReport += textContent.text;
            }
        }
        console.log(fullReport);
        break;
    } else if (["failed", "cancelled"].includes(interaction.status)) {
        console.log(`Failed: ${interaction.status}`);
        break;
    }
    await new Promise(resolve => setTimeout(resolve, 10000));
}
```

## Other Modalities and Specialized APIs

### Music Generation (Lyria)

Generate music from a text prompt, optionally combined with an image. Two models
are available:

-   `lyria-3-clip-preview` (**Lyria Clip**): Short clips up to 30 seconds.
-   `lyria-3-pro-preview` (**Lyria Pro**): Full-length tracks.

Both models use the same streaming API and return audio in the same format.

-   Use `generateContentStream` with `responseModalities: [Modality.AUDIO]`.
-   The response stream contains both audio (`inlineData`) and text (lyrics and
    metadata) parts. Accumulate the base64-encoded audio chunks, then decode
    into a playable Blob.
-   Unlike TTS and Live (which return raw PCM requiring `AudioContext`), these
    models return encoded audio (e.g., WAV). Use `atob` and `Blob` for playback.
-   When using Lyria models, users **MUST** select their own API key. Prompt
    them to set it in AI Studio Settings > Secrets > GEMINI_API_KEY before using
    these models.

#### Text-Only Music Generation

```ts
import { GoogleGenAI, Modality } from "@google/genai";
// ... initialization ...

const response = await ai.models.generateContentStream({
  model: "lyria-3-clip-preview", // or "lyria-3-pro-preview" for full-length tracks
  contents: 'Generate a 30-second cinematic orchestral track.',
});

let audioBase64 = "";
let lyrics = "";
let mimeType = "audio/wav";

for await (const chunk of response) {
  const parts = chunk.candidates?.[0]?.content?.parts;
  if (!parts) continue;

  for (const part of parts) {
    if (part.inlineData?.data) {
      if (!audioBase64 && part.inlineData.mimeType) {
        mimeType = part.inlineData.mimeType;
      }
      audioBase64 += part.inlineData.data;
    }
    if (part.text && !lyrics) {
      // The first text part contains the generated lyrics.
      lyrics = part.text;
    }
  }
}

// Decode base64 audio into a playable Blob URL
const binary = atob(audioBase64);
const bytes = new Uint8Array(binary.length);
for (let i = 0; i < binary.length; i++) {
  bytes[i] = binary.charCodeAt(i);
}
const blob = new Blob([bytes], { type: mimeType });
const audioUrl = URL.createObjectURL(blob);
```

#### Image + Text Music Generation

```ts
import { GoogleGenAI, Modality } from "@google/genai";
// ... initialization ...

const response = await ai.models.generateContentStream({
  model: "lyria-3-clip-preview", // or "lyria-3-pro-preview" for full-length tracks
  contents: {
    parts: [
      { text: 'Generate a 30-second track inspired by this image.' },
      { inlineData: { data: base64ImageData, mimeType: "image/jpeg" } },
    ],
  },
});
// Process the stream identically to the text-only example above.
```

### Video Generation and Editing (Omni Flash)

Generate and edit videos using the `gemini-omni-flash-preview` model.

Note: Video generation is a long-running operation. You can run it synchronously
(with a high timeout) or in the background (`background: true`) and poll for the
result.

#### 1. Text-to-Video (Synchronous)

Generate a video directly from a detailed prompt. Use `response_format` to
specify output parameters like aspect ratio and duration.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

const interaction = await ai.interactions.create({
  model: 'gemini-omni-flash-preview',
  input: 'A hyper-realistic close-up of an elephant drinking a large cup of tea.',
  background: false,
  store: false,
  stream: false,
  response_format: {
    type: 'video',
    aspect_ratio: '16:9', // '16:9' or '9:16'
    duration: '5s',       // '5s' or '10s'
  }
}, { timeout: 300000 }); // 5 minutes timeout

const videoPart = interaction.output_video;
if (videoPart && videoPart.data) {
  const videoBuffer = Buffer.from(videoPart.data, 'base64');
  // Save videoBuffer to file or send to client
}
```

#### 2. Image-to-Video (Animate Image)

Provide a starting frame image and a motion prompt.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

const interaction = await ai.interactions.create({
  model: 'gemini-omni-flash-preview',
  input: [
    {
      type: "image",
      mime_type: "image/png",
      data: base64ImageBytes,
    },
    {
      type: "text",
      text: "Cherry blossom petals gently fall while a breeze ripples through the garden pond.",
    }
  ],
  background: false,
  store: false,
  stream: false,
  response_format: {
    type: 'video',
    aspect_ratio: '16:9',
  }
}, { timeout: 300000 });

const videoPart = interaction.output_video;
if (videoPart && videoPart.data) {
  const videoBuffer = Buffer.from(videoPart.data, 'base64');
  // ...
}
```

#### 3. Storyboard Reference (Multiple Images)

Supply multiple reference images to guide the video generation. You can mix
stored interactions (via `previous_interaction_id`) and inline data.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

// Generate first reference image (stored)
const r1 = await ai.interactions.create({
  model: 'gemini-3.1-flash-lite-image',
  input: 'A cute tabby cat with a blue collar, portrait',
  store: true,
  response_format: { type: 'image' }
});

// Generate second reference image (not stored)
const r2 = await ai.interactions.create({
  model: 'gemini-3.1-flash-lite-image',
  input: 'A colorful ball of yarn on a wooden floor',
  store: false,
  response_format: { type: 'image' }
});
const yarnImage = r2.output_image;

// Generate video combining both
const interaction = await ai.interactions.create({
  model: 'gemini-omni-flash-preview',
  previous_interaction_id: r1.id, // References the cat image
  input: [
    {
      type: 'image',
      mime_type: yarnImage.mime_type,
      data: yarnImage.data
    },
    {
      type: 'text',
      text: 'The cat is playfully batting at the ball of yarn on a wooden floor.'
    }
  ],
  background: false,
  store: true,
  stream: false,
  response_format: {
    type: 'video',
    aspect_ratio: '16:9'
  }
}, { timeout: 300000 });

const videoPart = interaction.output_video;
// ...
```

#### 4. Video-to-Video Editing

Modify an existing video by providing the video and editing instruction. For
video inputs, it is recommended to use GCS URIs to avoid large payloads.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

const interaction = await ai.interactions.create({
  model: 'gemini-omni-flash-preview',
  input: [
    {
      type: "video",
      mime_type: "video/mp4",
      uri: "gs://cloud-samples-data/generative-ai/video/dog_day1.mp4",
    },
    {
      type: "text",
      text: "Change the dog to the cat. Add a propeller hat.",
    }
  ],
  background: false,
  store: false,
  stream: false,
  response_format: {
    type: 'video',
  }
}, { timeout: 300000 });

const videoPart = interaction.output_video;
// ...
```

#### 5. Multi-turn Video Editing (Chat)

To perform iterative edits, pass the `previous_interaction_id` of the previous
turn and the new instruction. The history is maintained automatically if `store:
true` was used.

```ts
import { GoogleGenAI } from "@google/genai";
// ... initialization ...

// Turn 1: Initial generation (must use store: true)
const interaction1 = await ai.interactions.create({
  model: 'gemini-omni-flash-preview',
  input: "A claymation Newton's First Law stop motion video.",
  background: false,
  store: true,
  stream: false,
  response_format: { type: 'video' }
}, { timeout: 300000 });

// Turn 2: Edit (only pass the new instruction and reference the previous turn)
const interaction2 = await ai.interactions.create({
  model: 'gemini-omni-flash-preview',
  previous_interaction_id: interaction1.id,
  input: "Now make it doodle style.",
  background: false,
  store: true, // Keep storing if you want to continue the conversation
  stream: false,
  response_format: { type: 'video' }
}, { timeout: 300000 });

const videoPart = interaction2.output_video;
// ...
```

### Live API

The Live API enables low-latency, real-time voice interactions with Gemini. The
server manages the Live API session; audio streams between the client and server
via WebSocket.

#### Session Setup & Audio Streaming

Server (`server.ts`): connects to Gemini Live and bridges audio via WebSocket.

```ts
import { GoogleGenAI, LiveServerMessage, Modality } from "@google/genai";
import { WebSocketServer } from "ws";

// ... initialization ...

wss.on("connection", async (clientWs) => {
  const session = await ai.live.connect({
    model: "gemini-3.1-flash-live-preview",
    callbacks: {
      onmessage: (message: LiveServerMessage) => {
        const audio = message.serverContent?.modelTurn?.parts[0]?.inlineData?.data;
        if (audio) clientWs.send(JSON.stringify({ audio }));
        if (message.serverContent?.interrupted)
          clientWs.send(JSON.stringify({ interrupted: true }));
      },
    },
    config: {
    responseModalities: [Modality.AUDIO], // Must be [Modality.AUDIO]
          speechConfig: {
        // 'Puck', 'Charon', 'Kore', 'Fenrir', 'Zephyr'
        voiceConfig: { prebuiltVoiceConfig: { voiceName: "Zephyr" } },
      },
      systemInstruction: "You are a helpful assistant.",
    },
  });

  clientWs.on("message", (data) => {
    const { audio } = JSON.parse(data.toString());
    session.sendRealtimeInput({
      audio: { data: audio, mimeType: "audio/pcm;rate=16000" },
    });
  });
});
```

Client (browser): captures mic audio and plays back responses.

```ts
const ws = new WebSocket(`ws://${location.host}/live`);
// Input: 16kHz for mic capture
const inputAudioCtx = new AudioContext({ sampleRate: 16000 });
// Output: 24kHz for model output playback
const outputAudioCtx = new AudioContext({ sampleRate: 24000 });

const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
const source = inputAudioCtx.createMediaStreamSource(stream);
const processor = inputAudioCtx.createScriptProcessor(4096, 1, 1);
source.connect(processor);
processor.connect(inputAudioCtx.destination);

processor.onaudioprocess = (e) => {
  const base64 = pcmToBase64(e.inputBuffer.getChannelData(0));
  ws.send(JSON.stringify({ audio: base64 }));
};

ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  if (msg.audio) playAudioChunk(outputAudioCtx, msg.audio); // Play back at 24kHz
  if (msg.interrupted) { /* stop playback, clear queue */ }
};
```

#### Live Translation

Use `gemini-3.5-live-translate-preview` for real-time speech translation.
Configure the target language using `translationConfig` in the session config.

Server (`server.ts`):

```ts
import { GoogleGenAI, LiveServerMessage, Modality } from "@google/genai";
import { WebSocketServer } from "ws";

// ... initialization ...

wss.on("connection", async (clientWs) => {
  const session = await ai.live.connect({
    model: "gemini-3.5-live-translate-preview",
    config: {
      responseModalities: [Modality.AUDIO],
      translationConfig: {
        targetLanguageCode: "es", // Translates spoken input audio into Spanish
        echoTargetLanguage: false // Default to false: do not parrot target-language input
      },
      systemInstruction: "You are a helpful assistant.",
    },
    callbacks: {
      onmessage: (message: LiveServerMessage) => {
        const audio = message.serverContent?.modelTurn?.parts[0]?.inlineData?.data;
        if (audio) clientWs.send(JSON.stringify({ audio }));
        if (message.serverContent?.interrupted)
          clientWs.send(JSON.stringify({ interrupted: true }));
      },
    },
  });

  clientWs.on("message", (data) => {
    const { audio } = JSON.parse(data.toString());
    session.sendRealtimeInput({
      audio: { data: audio, mimeType: "audio/pcm;rate=16000" },
    });
  });
});
```

#### Video Streaming

Stream image frames received from the client as separate inputs.

```ts
session.sendRealtimeInput(
  { video: { data: base64Data, mimeType: 'image/jpeg' } });
```

#### Audio Transcription

Enable transcription in config:

```ts
config: {
  // ...
  outputAudioTranscription: {}, // Model output
  inputAudioTranscription: {}, // User input
}
```

Handle `outputTranscription` and `inputTranscription` in `onmessage`.

#### Function Calling in Live

Define tools in config and handle `toolCall` in `onmessage`. Send
`functionResponses` back using `session.sendToolResponse`.

#### Live API Rules

-   **Streaming Input:** When sending data via `session.sendRealtimeInput`, use
    `audio`, `video`, or `text` fields. The `media` and `mediaChunks` fields are
    **deprecated** and **must not** be used.
    -   **Correct:** `session.sendRealtimeInput({ audio: ... })`
    -   **Correct:** `session.sendRealtimeInput({ video: ... })`
    -   **Incorrect:** `session.sendRealtimeInput({ media: ... })`
-   **Audio Sync:** Schedule audio chunks precisely for gapless playback. Track
    a `nextStartTime` and use `AudioBufferSourceNode.start(nextStartTime)` for
    each chunk, advancing by `buffer.duration`. Do not play chunks immediately
    on arrival — network jitter causes overlaps and gaps.
-   **Cleanup:** Use `session.close()` when finished.
-   **Modalities:** `responseModalities` must be `[Modality.AUDIO]`.
-   **Raw Audio:** Implement manual PCM encoding/decoding. Do not use
    `AudioContext.decodeAudioData` on raw streams.
-   **Session State:** Use `sessionPromise.then(...)` to send data to avoid race
    conditions and stale closures.
-   **Callback State (React):** In React, use `useRef` (not `useState`) for any
    variable accessed inside `onaudioprocess` or `setInterval` callbacks to
    avoid stale closures.
-   **Video:** Send synchronized image frames and audio.
-   **Multimodal Output:** Always handle audio output, even when transcription
    or function calls are enabled.
-   **Audio Formats:** Live API requires raw 16-bit PCM little-endian audio.
    Input audio must be 16kHz (`audio/pcm;rate=16000`), and model output audio
    is 24kHz. Ensure the client handles this (e.g., by using a separate 24kHz
    `AudioContext` for playback to avoid pitch-shifting or slow playback).
-   **Video Input Rate:** When streaming video/images, send JPEG frames at a
    rate of at most 1 frame per second (1 FPS) to avoid overwhelming the model.

### Embed Content

Generate embeddings for multimodal content (text, audio, image, and video).

```ts
import { GoogleGenAI } from "@google/genai";

// ... initialization ...

const result = await ai.models.embedContent({
  model: 'gemini-embedding-2-preview',
  contents: [
    'What is the meaning of life?',
    {
      inlineData: {
        data: base64ImageBytes,
        mimeType: 'image/png',
      },
    },
    {
      inlineData: {
        data: base64AudioBytes,
        mimeType: 'audio/mpeg',
      },
    },
  ],
});

console.log(result.embeddings);
```

### API Error Handling

-   Implement robust handling for API errors (e.g., network issues, 4xx/5xx
    responses).
-   Use graceful retry logic (like exponential backoff) to avoid overwhelming
    the backend and improve reliability.
-   Return meaningful error responses from API endpoints so the client can
    display appropriate messages.
-   If you receive a `PERMISSION_DENIED` (403) or `API_KEY_INVALID` (400) error
    from the Gemini API, or if the user reports this error in their application,
    ask the user to check the API key (or select one if not set) in the
    **Settings > Secrets** panel.
-   If you receive a `RESOURCE_EXHAUSTED` (429) error while using a free tier
    api key, it might be because you are accessing a paid-only model. Advise the
    user that upgrading to a paid tier increases their quota, and that they can
    use the **Settings > Secrets** panel to select a billing enabled API key.
-   If you encounter a `NOT_FOUND` (404) error when calling the Gemini API, or
    if the user reports a 404 error, it typically indicates that the requested
    model is invalid or unsupported. You **MUST** verify and update the model
    name to one of the active models defined in the **## @google/genai Models**
    section.
-   If you fail to parse the interaction output response (e.g., properties like
    `steps` or `output_text` are undefined or throw errors), check the SDK
    version in `package.json`. If the installed `@google/genai` version is
    `1.x.x` (or less than `2.4.0`), you **MUST** upgrade it to
    `@google/genai@latest` (or `^2.4.0`) by running `npm i @google/genai@latest`
    on the server, as legacy versions do not support the new steps schema.
