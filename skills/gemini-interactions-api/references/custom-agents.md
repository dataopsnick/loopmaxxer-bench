# Building Custom Agents Reference (JavaScript/TypeScript)

Managed agents on the Gemini API let you bundle instructions, skills, and an environment into a reusable agent that you can then invoke by ID.

## Run your first custom agent interaction

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
}, { timeout: 300000 });

console.log(result.output_text);
```

## Create a managed agent

### From sources
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

### From an existing environment (fork)
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

### Configuring network rules
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

## System instructions: AGENTS.md
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

## Skills: SKILL.md
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

## Invoke the agent

```javascript
const result = await ai.interactions.create({
    agent: "data-analyst",
    input: "Analyze Q1 revenue data from /workspace/templates/sample.csv and create a slide deck.",
    environment: "remote",
}, { timeout: 300000 });

console.log(result.output_text);
```

### Overriding configuration at invocation
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

## Manage agents

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
