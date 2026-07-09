---
name: gemini-interactions-api
description: Instructions for the Gemini Interactions API components, covering Agent Environments, the Antigravity Agent, Build Patterns for Custom Agents, Deep Research, and general Managed Agent configurations.

---

skills/
└── system_skills/
    ├── gemini-interactions-api/
    │   ├── SKILL.md
    │   └── references/
    │       ├── agent-environment.md
    │       ├── antigravity-agent.md
    │       ├── custom-agents.md
    │       ├── deep-research.md
    │       └── managed-agents.md

## Agent Environments Reference

Environments are managed Linux sandboxes that give agents an isolated place to execute code and persist files. They are decoupled from interaction context, so you can reuse the same environment across multiple interactions or start fresh.

### Provision a fresh sandbox and retrieve ID

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Install pandas and matplotlib, verify the imports, and print the versions.",
    environment: "remote",
});

console.log(`Environment ID: ${interaction.environment_id}`);
```

### Environment parameter usage patterns

The `environment` parameter supports three forms: `"remote"` (fresh sandbox), an environment ID string (reuse existing sandbox), or a config object (declarative custom setup).

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

// 1. Fresh sandbox
const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Write a hello world script.",
    environment: "remote",
});

// 2. Reuse an existing sandbox (preserves files and packages)
const interaction2 = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Modify the script to accept a name argument.",
    environment: interaction.environment_id,
    previous_interaction_id: interaction.id,
});

// 3. New sandbox with declaratively mounted sources
const interaction3 = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "List all files and summarize the project.",
    environment: {
        type: "remote",
        sources: [
            {
                type: "repository",
                source: "https://github.com/octocat/Spoon-Knife",
                target: "/workspace/spoon-knife",
            },
        ],
    },
});

console.log(interaction.output_text);
```

### Configure and reuse environments

Mount files/packages interactively, then capture the `environment_id` and reuse it for subsequent operations:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Install pandas, matplotlib, and seaborn. Verify all imports work and print the installed versions.",
    environment: "remote",
});

const interaction2 = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Clone https://github.com/octocat/Spoon-Knife into /workspace/tools. Run the test suite and fix any missing dependencies.",
    environment: interaction.environment_id,
    previous_interaction_id: interaction.id,
});

const interaction3 = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Using the tools in /workspace/tools, list the files.",
    environment: interaction.environment_id,
    previous_interaction_id: interaction2.id,
});
console.log(interaction.output_text);
```

### Mount custom sources at startup

You can declaratively mount sources (repositories, Cloud Storage, or raw inline text) into the sandbox at startup. You cannot set the root directory (`/`) as a target; you must always specify a subdirectory.

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "List all files under /workspace and describe what you find.",
    environment: {
        type: "remote",
        sources: [
            {
                type: "repository",
                source: "https://github.com/octocat/Spoon-Knife",
                target: "/workspace/spoon-knife",
            },
            {
                type: "gcs",
                source: "gs://cloud-samples-data/bigquery/us-states/",
                target: "/workspace/gcs-data",
            },
            {
                type: "inline",
                content: "# Project Notes\n\n- Analyze state population data\n- Create visualizations\n",
                target: "/workspace/notes/readme.md",
            },
        ],
    },
});

console.log(interaction.output_text);
```

### Private repository downloads (with credentials)

Configure outbound credentials to pull private repositories inside the sandbox. Encrypt GitHub Personal Access Tokens (PATs) using `Basic` authentication where username is `x-oauth-basic`.

```javascript
// Encode: echo -n "x-oauth-basic:ghp_YourPATHere" | base64
const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Run the test for my backend app and fix any issue.",
    environment: {
        type: "remote",
        sources: [
            {
                type: "repository",
                source: "https://github.com/your-org/backend",
                target: "/backend-app"
            }
        ],
        network: {
            allowlist: [
                {
                    domain: "github.com",
                    transform: {
                        "Authorization": "Basic YOUR_BASE64_TOKEN"
                    }
                },
                {
                    domain: "*"
                }
            ]
        }
    },
});
```

### Private Cloud Storage downloads (with credentials)

```javascript
// Retrieve: gcloud auth print-access-token
const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Analyze the discrepancies across the data in workspace",
    environment: {
        type: "remote",
        sources: [
            {
                type: "gcs",
                source: "gs://my-private-bucket/data",
                target: "/workspace",
            }
        ],
        network: {
            allowlist: [
                {
                    domain: "storage.googleapis.com",
                    transform: {
                        "Authorization": "Bearer YOUR_GCS_TOKEN"
                    }
                },
                {
                    domain: "*"
                }
            ]
        }
    },
});
```

### Network egress configurations

By default, environments have unrestricted outbound network access. You can use the `network` parameter to restrict outbound traffic to specific domains.

#### Credentials injection using transform
You can inject credentials (such as OAuth 2.0 Bearer tokens) into matching requests dynamically through header transformations on listed allowlist domains.

```javascript
import { GoogleGenAI } from "@google/genai";
import { execSync } from "child_process";

const gcloudToken = execSync("gcloud auth print-access-token").toString().trim();

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "List the files in gs://my-bucket/reports/ using the GCS JSON API.",
    environment: {
        type: "remote",
        network: {
            allowlist: [
                {
                    domain: "storage.googleapis.com",
                    transform: {
                        "Authorization": `Bearer ${gcloudToken}`
                    },
                }
            ]
        }
    },
});

console.log(interaction.output_text);
```

#### Disable network traffic entirely
To isolate the sandbox and block all outbound network access, set `network` to `"disabled"`:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Analyze the local files only.",
    environment: {
        type: "remote",
        network: "disabled",
    },
});

console.log(interaction.output_text);
```

### Download files from the environment snapshot

Use the Files API to download the full environment snapshot workspace as a tar file.

```javascript
import { GoogleGenAI } from "@google/genai";
import { execSync } from "child_process";
import * as fs from "fs";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Write a file environments_test.txt with content 'Environments' inside the sandbox.",
    environment: "remote",
});

const envId = interaction.environment_id;
const apiKey = process.env.GEMINI_API_KEY || "";

const url = `https://generativelanguage.googleapis.com/v1beta/files/environment-${envId}:download?alt=media`;
const response = await fetch(url, {
    headers: {
        "x-goog-api-key": apiKey,
    },
});

if (!response.ok) {
    throw new Error(`Failed to download file: ${response.statusText}`);
}

const buffer = Buffer.from(await response.arrayBuffer());
fs.writeFileSync("snapshot_env.tar", buffer);

if (!fs.existsSync("extracted_env_snapshot")) {
    fs.mkdirSync("extracted_env_snapshot");
}
execSync("tar -xf snapshot_env.tar -C extracted_env_snapshot");

console.log(fs.readdirSync("extracted_env_snapshot"));
```

---

## Antigravity Agent Reference

The Antigravity agent (`antigravity-preview-05-2026`) is a general-purpose managed agent on the Gemini API. A single API call gives you an agent that reasons, executes code, manages files, and browses the web inside your own secure Linux sandbox, hosted by Google.

### Run your first agent interaction

By default, the agent has access to `code_execution`, `google_search`, and `url_context`.

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Read Hacker News, summarize the top 10 stories, and save the results as a PDF.",
    environment: "remote",
}, { timeout: 300000 });

console.log(interaction.output_text);
```

### Restricting Tools

To limit the agent to specific tools, pass only the ones you need in the `tools` parameter:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Search for the latest AI research papers on reasoning and summarize them.",
    environment: "remote",
    tools: [
        { type: "google_search" },
        { type: "url_context" },
    ],
}, { timeout: 300000 });

console.log(interaction.output_text);
```

### Multimodal Input

The Antigravity agent supports multimodal inputs (text and image). Images must be supplied as inline base64-encoded strings (`data`).

```javascript
import { GoogleGenAI } from "@google/genai";
import * as fs from "node:fs";

const ai = new GoogleGenAI({});
const base64Image = fs.readFileSync("path/to/chart.png", { encoding: "base64" });

const interactionInline = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: [
        { type: "text", text: "Analyze this chart and summarize the trends." },
        {
            type: "image",
            data: base64Image,
            mime_type: "image/png",
        },
    ],
    environment: "remote",
}, { timeout: 300000 });
```

---

## Building Custom Agents Reference

Managed agents on the Gemini API let you bundle instructions, skills, and an environment into a reusable agent that you can then invoke by ID.

### Run your first custom agent interaction

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

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

const result = await ai.interactions.create({
    agent: "code-reviewer",
    input: "Review the latest changes in /workspace/repo/src and file a summary.",
    environment: "remote",
keys: true,
}, { timeout: 300000 });

console.log(result.output_text);
```

### Create a managed agent

#### From sources
Specify `system_instruction` and `base_environment` with sources (inline, repository, etc.):

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const agent = await ai.agents.create({
    id: "data-analyst",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You are a data analyst. Always include visualizations and export results as PDF.",
    base_environment: {
        type: "remote",
        sources: [
            {
                type: "inline",
                target: ".agents/AGENTS.md",
                content: "Always use matplotlib for charts. Include a summary table in every report.",
            },
            {
                type: "inline",
                target: ".agents/skills/slide-maker/SKILL.md",
                content: "---\nname: slide-maker\n---\n# Slide Maker\nCreate HTML slide decks from data analysis results.",
            },
            {
                type: "repository",
                source: "https://github.com/my-org/analysis-templates",
                target: "/workspace/templates",
            },
        ],
    },
});

console.log(`Created agent: ${agent.id}`);
```

#### From an existing environment (fork)
Iterate with the base Antigravity agent until the environment is correct, then fork it into a managed agent:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Install pandas, matplotlib, and seaborn. Create an analysis template at /workspace/template.py.",
    environment: "remote",
}, { timeout: 300000 });

const agent = await ai.agents.create({
    id: "my-data-analyst",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You are a data analyst. Use the template at /workspace/template.py for all reports.",
    base_environment: interaction.environment_id,
});

console.log(`Forked agent successfully: ${agent.id}`);
```

#### Configuring network rules
Use the `network` field to restrict outbound traffic to specific domains:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const agent = await ai.agents.create({
    id: "issue-resolver",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You resolve GitHub issues. Clone the repo, find the bug, write the fix, run the tests, and open a PR.",
    base_environment: {
        type: "remote",
        sources: [
            {
                type: "repository",
                source: "https://github.com/my-org/backend",
                target: "/workspace/repo",
            }
        ],
        network: {
            allowlist: [
                {
                    domain: "api.github.com",
                    transform: {
                        "Authorization": "Basic YOUR_BASE64_TOKEN"
                    },
                },
                { domain: "pypi.org" },
            ]
        }
    },
});

console.log(`Created issue-resolver agent successfully: ${agent.id}`);
```

### System instructions: AGENTS.md
Mount an `AGENTS.md` using an inline source:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const agent = await ai.agents.create({
    id: "styled-writer",
    base_agent: "antigravity-preview-05-2026",
    base_environment: {
        type: "remote",
        sources: [
            {
                type: "inline",
                target: ".agents/AGENTS.md",
                content: "# Writing Style\n\n- Use active voice\n- Keep paragraphs under 3 sentences\n- Include code examples for every concept",
            },
        ],
    },
});

console.log(`Created styled-writer agent: ${agent.id}`);
```

### Skills: SKILL.md
Mount a skill using an inline source:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const agent = await ai.agents.create({
    id: "presenter",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You create presentations from data.",
    base_environment: {
        type: "remote",
        sources: [
            {
                type: "inline",
                target: ".agents/skills/slide-maker/SKILL.md",
                content: "---\nname: slide-maker\ndescription: Create HTML slide decks\n---\n# Slide Maker\n\nWhen asked to create a presentation:\n1. Analyze the input data\n2. Create an HTML slide deck with reveal.js\n3. Save to /workspace/output/slides.html",
            },
        ],
    },
});

console.log(`Created presenter: ${agent.id}`);
```

### Invoke the agent

```javascript
const result = await ai.interactions.create({
    agent: "data-analyst",
    input: "Analyze Q1 revenue data from /workspace/templates/sample.csv and create a slide deck.",
    environment: "remote",
}, { timeout: 300000 });

console.log(result.output_text);
```

#### Overriding configuration at invocation
Override the agent's default `system_instruction` and `tools` when creating an interaction:

```javascript
const result = await ai.interactions.create({
    agent: "data-analyst",
    input: "Analyze Q1 revenue data, but do not create a slide deck. Just output a summary table.",
    system_instruction: "You are a data analyst. Focus ONLY on summary tables. Ignore default instructions about slides.",
    tools: [{ type: "code_execution" }], // Override to only use code execution
    environment: "remote",
}, { timeout: 300000 });

console.log(result.output_text);
```

### Manage agents

```javascript
// List agents
const agents = await ai.agents.list();
if (agents.agents) {
    for (const a of agents.agents) {
        console.log(`${a.id}: ${a.description}`);
    }
}

// Get agent details
const agent = await ai.agents.get("data-analyst");
console.log(agent);

// Delete agent configuration
await ai.agents.delete("data-analyst");
```

---

## Deep Research Agent Reference

The Gemini Deep Research Agent autonomously plans, executes, and synthesizes multi-step research tasks. Research tasks can take several minutes to complete, so they must be run in the background (`background: true`) and polled or streamed.

### Basic Research (Background + Polling)

```javascript
import { GoogleGenAI } from '@google/genai';

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    input: 'Research the history of Google TPUs.',
    agent: 'deep-research-preview-04-2026',
    background: true
});

console.log(`Research started: ${interaction.id}`);

while (true) {
    const result = await ai.interactions.get(interaction.id);
    if (result.status === 'completed') {
        console.log(getFullOutputText(result));
        break;
    } else if (result.status === 'failed') {
        console.log(`Research failed: ${result.error}`);
        break;
    }
    await new Promise(resolve => setTimeout(resolve, 10000));
}

// Helper function to extract full report text from all steps
function getFullOutputText(interaction) {
    let text = "";
    for (const step of interaction.steps) {
        if (step.type === 'model_output') {
            const textContent = step.content?.find(c => c.type === 'text');
            if (textContent) text += textContent.text;
        }
    }
    return text;
}
```

### Collaborative Planning

Allows you to review and refine the research plan before execution.

> [!IMPORTANT]
> **Dynamic Thread Continuation**:
> Every plan adjustment (follow-up) or plan approval creates a **new child interaction** with its own distinct `interaction.id` (chained via `previous_interaction_id`).
> When the user submits feedback or approves the plan, your application must update its active session/interaction ID to this newly returned ID on both the server and client. Continuing to poll or check the initial planning interaction ID will make the UI appear stuck or hung, as that planning step is already marked as completed.

#### Step 1: Request a plan
Set `collaborative_planning: true` in the `agent_config` to receive a plan:

```javascript
const planInteraction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Do some research on Google TPUs.',
    agent_config: {
        type: 'deep-research',
        thinking_summaries: 'auto',
        collaborative_planning: true
    },
    background: true
});

let result;
while ((result = await ai.interactions.get(planInteraction.id)).status !== 'completed') {
    await new Promise(r => setTimeout(r, 5000));
}
console.log(getFullOutputText(result));
```

#### Step 2: Refine the plan (optional)
Iterate on the plan using `previous_interaction_id`:

```javascript
const refinedPlan = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Focus more on the differences between Google TPUs and competitor hardware, and less on the history.',
    agent_config: {
        type: 'deep-research',
        thinking_summaries: 'auto',
        collaborative_planning: true
    },
    previous_interaction_id: planInteraction.id,
    background: true
});

let result;
while ((result = await ai.interactions.get(refinedPlan.id)).status !== 'completed') {
    await new Promise(r => setTimeout(r, 5000));
}
console.log(getFullOutputText(result));
```

#### Step 3: Approve and execute
Approve the plan by setting `collaborative_planning: false` (or omitting it):

```javascript
const finalReport = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Plan looks good!',
    agent_config: {
        type: 'deep-research',
        thinking_summaries: 'auto',
        collaborative_planning: false
    },
    previous_interaction_id: refinedPlan.id,
    background: true
});

let result;
while ((result = await ai.interactions.get(finalReport.id)).status !== 'completed') {
    await new Promise(r => setTimeout(r, 5000));
}
console.log(getFullOutputText(result));
```

### Visualization

Set `visualization: "auto"` to allow the agent to generate charts and graphs:

```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Analyze global semiconductor market trends. Include graphics showing market share changes.',
    agent_config: {
        type: 'deep-research',
        visualization: 'auto'
    },
    background: true
});

let result;
while ((result = await ai.interactions.get(interaction.id)).status !== 'completed') {
    await new Promise(r => setTimeout(r, 5000));
}

for (const step of result.steps) {
    if (step.type === 'model_output') {
        for (const contentItem of step.content) {
            if (contentItem.type === 'text') {
                console.log(contentItem.text);
            } else if (contentItem.type === 'image' && contentItem.data) {
                console.log(`[Image Output: ${contentItem.data.substring(0, 20)}...]`);
            }
        }
    }
}
```

### Restricting Tools

By default, the agent has access to Google Search, URL Context, and Code Execution.

#### Google Search Only
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'What are the latest developments in quantum computing?',
    tools: [{ type: 'google_search' }],
    background: true
});
```

#### URL Context Only
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Summarize the content of https://www.wikipedia.org/.',
    tools: [{ type: 'url_context' }],
    background: true
});
```

#### Code Execution Only
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Calculate the 50th Fibonacci number.',
    tools: [{ type: 'code_execution' }],
    background: true
});
```

#### Connecting MCP Servers
Pass the MCP server configuration in `tools`:

```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Check the status of my last server deployment.',
    tools: [
        {
            type: 'mcp_server',
            name: 'Deployment Tracker',
            url: 'https://mcp.example.com/mcp',
            headers: { Authorization: 'Bearer my-token' }
        }
    ],
    background: true
});
```

#### File Search
Give the agent access to document corpora stores:

```javascript
const interaction = await ai.interactions.create({
    input: 'Compare our 2025 fiscal year report against current public web news.',
    agent: 'deep-research-preview-04-2026',
    background: true,
    tools: [
        { type: 'file_search', file_search_store_names: ['fileSearchStores/my-store-name'] },
    ]
});
```

### Steerability and formatting

Include explicit guidelines in your prompt to control report structure:

```javascript
const prompt = `
Research the competitive landscape of EV batteries.

Format the output as a technical report with the following structure:
1. Executive Summary
2. Key Players (Must include a data table comparing capacity and chemistry)
3. Supply Chain Risks
`;

const interaction = await ai.interactions.create({
    input: prompt,
    agent: 'deep-research-preview-04-2026',
    background: true,
});
```

### Multimodal Inputs

#### Images
```javascript
const prompt = `Analyze the interspecies dynamics and behavioral risks in this image.`;

const interaction = await ai.interactions.create({
    input: [
        { type: 'text', text: prompt },
        {
            type: 'image',
            mime_type: "image/jpeg",
            uri: 'https://storage.googleapis.com/generativeai-downloads/images/generated_elephants_giraffes_zebras_sunset.jpg'
        }
    ],
    agent: 'deep-research-preview-04-2026',
    background: true
});
```

#### Documents (PDFs)
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: [
        { type: 'text', text: 'What is this document about?' },
        {
            type: 'document',
            uri: 'https://arxiv.org/pdf/1706.03762',
            mime_type: 'application/pdf'
        }
    ],
    background: true
});
```

### Streaming with Reconnection

Check status and resume from a saved `lastEventId` if the connection drops.

```javascript
import { GoogleGenAI } from '@google/genai';

const ai = new GoogleGenAI({});

let interactionId;
let lastEventId;
let isComplete = false;

async function processStream(stream) {
    for await (const event of stream) {
        if (event.type === 'interaction.created') {
            interactionId = event.interaction.id;
        }
        if (event.event_id) lastEventId = event.event_id;
        if (event.type === 'step.delta') {
            if (event.delta.type === 'text') {
                process.stdout.write(event.delta.text);
            } else if (event.delta.type === 'thought') {
                console.log(`Thought: ${event.delta.text}`);
            }
        } else if (['interaction.completed', 'error'].includes(event.type)) {
            isComplete = true;
        }
    }
}

const stream = await ai.interactions.create({
    input: 'Research the history of Google TPUs.',
    agent: 'deep-research-preview-04-2026',
    background: true,
    stream: true,
    agent_config: { type: 'deep-research', thinking_summaries: 'auto' },
});
await processStream(stream);

while (!isComplete && interactionId) {
    const status = await ai.interactions.get(interactionId);
    if (status.status !== 'in_progress') break;
    const resumeStream = await ai.interactions.get(interactionId, {
        stream: true, last_event_id: lastEventId,
    });
    await processStream(resumeStream);
}
```

### Continuing Conversation

Ask follow-up questions to clarify or expand sections without restarting:

```javascript
const interaction = await ai.interactions.create({
    input: 'Can you elaborate on the second point in the report?',
    model: 'gemini-3.1-pro-preview',
    previous_interaction_id: 'COMPLETED_INTERACTION_ID'
});
console.log(getFullOutputText(interaction));
```

---

## Managed Agents Reference

This reference guide provides JavaScript/TypeScript code patterns for creating and using Managed Agents with the `@google/genai` SDK, using the Antigravity agent (`antigravity-preview-05-2026`).

### Run your first agent interaction

A single call to `ai.interactions.create` provisions a Linux sandbox, runs the agent loop, and returns the result.

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const interaction = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Write a Python script that generates the first 20 Fibonacci numbers and saves them to fibonacci.txt. Then read the file and print its contents.",
    environment: "remote",
});

console.log(`Interaction ID: ${interaction.id}`);
console.log(`Environment ID: ${interaction.environment_id}`);
console.log(`Output: ${interaction.output_text}`);
```

### Continue the conversation (multi-turn)

The API tracks both conversation context (using `previous_interaction_id`) and environment state (using `environment`). Pass both to resume the conversation in the same sandbox:

```javascript
const interaction2 = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    previous_interaction_id: interaction.id,
    environment: interaction.environment_id,
    input: "Now plot the Fibonacci sequence as a line chart and save it as chart.png.",
}, { timeout: 300_000 });

console.log(interaction2.output_text);
```

You can mix and match these states independently:
- **Clear conversation, keep files:** Omit `previous_interaction_id`, only pass the environment ID using `environment` for a fresh conversation in the same workspace.
- **Keep conversation, new workspace:** Pass `previous_interaction_id`, set `environment="remote"` for a fresh sandbox.

### Stream the response

For long-running tasks, you can stream the response by setting `stream: true`, to see the agent work in real time:

```javascript
import { GoogleGenAI } from "@google/genai";

const ai = new GoogleGenAI({});

const stream = await ai.interactions.create({
    agent: "antigravity-preview-05-2026",
    input: "Read Hacker News, summarize the top 5 stories, and save the results as a PDF.",
    environment: "remote",
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

### Download files from the environment

The agent can create files inside the sandbox. To download them, fetch the download URL using the Files API:

```javascript
import fs from "fs";
import { execSync } from "child_process";

const envId = interaction.environment_id;
const apiKey = process.env.GEMINI_API_KEY || "";

const url = `https://generativelanguage.googleapis.com/v1beta/files/environment-${envId}:download?alt=media`;
const response = await fetch(url, {
    headers: {
        "x-goog-api-key": apiKey,
    },
});

if (!response.ok) {
    throw new Error(`Failed to download file: ${response.statusText}`);
}

const buffer = Buffer.from(await response.arrayBuffer());
fs.writeFileSync("snapshot.tar", buffer);

if (!fs.existsSync("extracted_snapshot")) {
    fs.mkdirSync("extracted_snapshot");
}
execSync("tar -xf snapshot.tar -C extracted_snapshot");

console.log(fs.readdirSync("extracted_snapshot"));
```

### Build a custom agent

You can create a custom agent by bundling instructions, tools, and environment sources into a named agent.

#### From sources
Define sources inline, or from other sources such as GitHub or Cloud Storage:

```javascript
const agent = await ai.agents.create({
    id: "fibonacci-analyst",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You are a math analysis agent. Generate sequences, visualize them, and export results as PDF reports.",
    base_environment: {
        type: "remote",
        sources: [
            {
                type: "inline",
                target: ".agents/AGENTS.md",
                content: "Always include a chart and a summary table in your reports.",
            },
            {
                type: "repository",
                source: "https://github.com/your-org/skills",
                target: ".agents/skills"
            }
        ],
    },
});

console.log(`Created agent: ${agent.id}`);
```

#### From an existing environment
Iterate on an environment through interactions, and then set it as the base for your agent referencing its ID:

```javascript
const agent = await ai.agents.create({
    id: "fibonacci-analyst",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You are a math analysis agent.",
    base_environment: interaction.environment_id,
});
```

### Invoke the custom agent

Once you've created a custom agent, you can invoke it by name instead of repeating configuration. Each invocation forks the base environment, so every run starts clean:

```javascript
const result = await ai.interactions.create({
    agent: "fibonacci-analyst",
    input: "Generate the first 50 prime numbers, plot their distribution, and save a PDF report.",
    environment: "remote",
}, {
    timeout: 300_000,
});

console.log(result.output_text);
```
