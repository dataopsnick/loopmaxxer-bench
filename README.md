# LOOPMAXXER-bench

Creating a standard benchmark of loop engineering tasks to measure the asymptotic performance of different Generative AI models on long-running tasks when operating within an agentic code harness.

## **Cloud Sandbox**

While it is a common assumption that an LLM API requires external infrastructure to execute code, the **Gemini Interactions API** is distinct because it includes a built-in, fully managed container orchestration system. 

---

### 1. How Gemini's Built-In Sandbox Works
When you invoke a managed agent like `antigravity-preview-05-2026` via the Interactions API, specifying `environment: "remote"` tells Google to automatically provision an isolated, ephemeral Linux container in their cloud. 

Within this environment, the agent is already equipped to:
* **Write and Run Code:** The agent can write scripts (Python, Node.js, Bash) and execute them natively inside its container.
* **Manage Packages & Dependencies:** It can run `pip install` or `npm install` to dynamically configure its workspace [references/managed-agents.md].
* **Persist State:** By capturing the returned `environment_id` and passing it to subsequent turns, you can maintain the exact same container state (including files, packages, and history) over multiple calls.
* **Declaratively Mount Sources:** You can feed private repos, Cloud Storage buckets, or raw file content directly into the container using the declarative `sources` parameter during the initial API call [references/agent-environment.md].

For standard workflows using Gemini's managed agents, **no external sandbox provider like Daytona is required** to execute code. 

---

### 2. Why Daytona is Still Highly Useful
Even though Gemini has built-in sandboxing, Daytona serves several critical purposes that the native Gemini sandbox does not cover:

#### A. Multi-Model and Framework Independence
Gemini's built-in sandbox only works if you are calling Gemini's managed agents (`antigravity` or `deep-research`). If you want to use a standard base model (like `gemini-3.5-flash`), or if your application switches between different LLM providers (such as Anthropic, OpenAI, or local models), you cannot use Google's managed agent sandbox. Daytona provides a uniform, provider-agnostic computer interface to run generated code regardless of which model created it.

#### B. Fine-Grained Infrastructure Control
Gemini's hosted sandbox is highly managed and relatively opaque. In contrast, Daytona provides deep programmatic control over the sandbox's system-level attributes:
* **Custom Hardware Allocations:** You can explicitly allocate CPU, memory, and disk sizing (e.g., `cpu=1`, `memory=1`, `disk=3`) [examples/python/declarative-image/main.py].
* **Persistent Volumes:** You can create separate, persistent storage volumes and mount them across different sandbox instances (useful for multi-tenant data isolation) [examples/python/volumes/volume.py].
* **Interactive Terminal Sessions (PTY):** Daytona allows you to create active pseudo-terminal (PTY) sessions to stream real-time stdin/stdout, interactive commands, and resize terminals dynamically [examples/python/pty/main.py].
* **Language Server Protocol (LSP):** You can run language servers (like TypeScript or Python LSP) directly inside a Daytona sandbox to fetch autocompletions or navigate code structures [examples/python/git-lsp/main.py].

#### C. Hybrid Architectures
Because of these differences, a common design pattern involves using standard Gemini models for reasoning, while offloading the execution, file system management, and environment orchestration to Daytona. For instance, you can use the Daytona ADK plugin to connect a base model to Daytona sandboxes for secure code execution.

---

### Summary Comparison

| Feature | Gemini Managed Sandbox (Interactions API) | Daytona Sandbox |
| :--- | :--- | :--- |
| **Orchestration** | Managed automatically by Google under the hood. | Programmatically controlled via the Daytona SDK. |
| **Model Coupling** | Tied strictly to Gemini's managed agents. | Model-agnostic (works with any LLM provider). |
| **Custom Sizing** | Opaque/fixed compute resources. | Configurable CPU, memory, disk, and custom regions [examples/python/declarative-image/main.py, examples/python/region/main.py]. |
| **Advanced Tools** | Limited to basic CLI, code runs, and browser tools. | Interactive PTY, LSP support, custom network policies, and persistent volumes [examples/python/git-lsp/main.py, examples/python/pty/main.py, examples/python/volumes/volume.py]. |

## **Local Cloud Emulators**.

---

### 1. How they integrate with Daytona
Daytona is an excellent environment for running local cloud emulators like LocalStack or Floci. 

Because a Daytona sandbox is a full, highly-configurable Linux workspace with Docker support, you can seamlessly deploy these emulators inside them. For example, in a Daytona workspace, your agent can:
1. Spin up **Floci** or **LocalStack** inside the sandbox using Docker.
2. Write and apply Terraform code targeting the local endpoint (`http://localhost:4566`).
3. Run and debug AWS Lambda functions, write to mocked S3 buckets, or populate mocked DynamoDB tables.
4. Tear down the entire Daytona sandbox once testing is complete, leaving zero footprint.

---

### 2. How they relate to the Gemini Interactions API
Running LocalStack or Floci directly inside the **Gemini Interactions API's** managed sandbox (`environment: "remote"`) is possible but comes with some constraints:
* **Docker Dependencies:** Both emulators run stateful services (like RDS or Lambda) by spinning up real Docker containers under the hood. If the Google-managed container in the Interactions API lacks nested Docker/virtualization capabilities, executing complex, stateful emulations inside it may fail.
* **API Calls & Basic Mocking:** If your agent only needs to mock simple API integrations (like making standard S3 or DynamoDB API calls), you can download the lightweight binaries directly into the Interactions sandbox and point the agent's SDK to `localhost:4566`.

---

### 3. LocalStack vs. Floci for AI Agents

While LocalStack has long been the industry standard for AWS emulation, **Floci** was specifically designed to solve some of the friction points that make LocalStack difficult to use with ephemeral AI agents.

| Metric / Feature | LocalStack (Community) | Floci (`floci.io`) |
| :--- | :--- | :--- |
| **Authentication** | Requires auth tokens/sign-ups (as of March 2026). | **Zero tokens or credentials required**. |
| **Startup Time** | ~3.3 seconds. | **~24 milliseconds** (built with Quarkus and GraalVM Native). |
| **Idle Memory Usage**| ~143 MiB. | **~13 MiB**. |
| **Docker Image Size** | ~1.0 GB. | **~90 MB**. |
| **License** | Restricted / Proprietary Community Split. | **MIT License (Always Free)**. |

#### Why Floci is optimized for AI Agents:
Because automated AI agents regularly spin up short-lived, ephemeral sandboxes to verify code, they need dependencies to load instantly and use minimal resources. 
* **Zero Token Injection:** Injecting persistent LocalStack developer tokens into thousands of short-lived agent sandboxes is a security and configuration chore. Floci requires no authentication, meaning an agent can simply download, boot, and use it immediately with zero configuration.
* **Resource Preservation:** Running a 1 GB container that takes several seconds to boot and eats 143 MiB of idle RAM is heavy for quick automated runs. Floci's tiny footprint (~90MB image, 24ms startup) makes it highly practical to bundle directly inside short-lived testing environments.
