# Managed Agents Reference (JavaScript/TypeScript)

This reference guide provides JavaScript/TypeScript code patterns for creating and using Managed Agents with the `@google/genai` SDK, using the Antigravity agent (`antigravity-preview-05-2026`).

## Run your first agent interaction

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

## Continue the conversation (multi-turn)

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

## Stream the response

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

## Download files from the environment

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

## Build a custom agent

You can create a custom agent by bundling instructions, tools, and environment sources into a named agent.

### From sources
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

### From an existing environment
Iterate on an environment through interactions, and then set it as the base for your agent referencing its ID:

```javascript
const agent = await ai.agents.create({
    id: "fibonacci-analyst",
    base_agent: "antigravity-preview-05-2026",
    system_instruction: "You are a math analysis agent.",
    base_environment: interaction.environment_id,
});
```

## Invoke the custom agent

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
