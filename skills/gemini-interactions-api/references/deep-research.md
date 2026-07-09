# Deep Research Agent Reference (JavaScript/TypeScript)

The Gemini Deep Research Agent autonomously plans, executes, and synthesizes multi-step research tasks. Research tasks can take several minutes to complete, so they must be run in the background (`background: true`) and polled or streamed.

## Basic Research (Background + Polling)

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

## Collaborative Planning

Allows you to review and refine the research plan before execution.

> [!IMPORTANT]
> **Dynamic Thread Continuation**:
> Every plan adjustment (follow-up) or plan approval creates a **new child interaction** with its own distinct `interaction.id` (chained via `previous_interaction_id`).
> When the user submits feedback or approves the plan, your application must update its active session/interaction ID to this newly returned ID on both the server and client. Continuing to poll or check the initial planning interaction ID will make the UI appear stuck or hung, as that planning step is already marked as completed.

### Step 1: Request a plan
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

### Step 2: Refine the plan (optional)
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

### Step 3: Approve and execute
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

## Visualization

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

## Restricting Tools

By default, the agent has access to Google Search, URL Context, and Code Execution.

### Google Search Only
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'What are the latest developments in quantum computing?',
    tools: [{ type: 'google_search' }],
    background: true
});
```

### URL Context Only
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Summarize the content of https://www.wikipedia.org/.',
    tools: [{ type: 'url_context' }],
    background: true
});
```

### Code Execution Only
```javascript
const interaction = await ai.interactions.create({
    agent: 'deep-research-preview-04-2026',
    input: 'Calculate the 50th Fibonacci number.',
    tools: [{ type: 'code_execution' }],
    background: true
});
```

### Connecting MCP Servers
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

### File Search
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

## Steerability and formatting

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

## Multimodal Inputs

### Images
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

### Documents (PDFs)
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

## Streaming with Reconnection

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

## Continuing Conversation

Ask follow-up questions to clarify or expand sections without restarting:

```javascript
const interaction = await ai.interactions.create({
    input: 'Can you elaborate on the second point in the report?',
    model: 'gemini-3.1-pro-preview',
    previous_interaction_id: 'COMPLETED_INTERACTION_ID'
});
console.log(getFullOutputText(interaction));
```
