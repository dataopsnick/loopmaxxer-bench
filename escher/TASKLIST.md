<tasklist>
  <task status="NOT STARTED">
    <id>1</id>
    <title>loopmaxxer-bench/escher/proxy.py - Enable OpenRouter Context Compression Plugin</title>
    <description><![CDATA[
### Location: escher/proxy.py

[bug · high] Fix `400 Bad Request` crashes during Open Code Review (OCR) when analyzing large codebases by enabling OpenRouter's context compression plugin.

#### Issue:
When `ocr` parses a large repository, the generated prompt can exceed the model's maximum context length (e.g., exceeding 1,048,576 tokens). OpenRouter rejects this with a memory compression failure (`400 Bad Request`), which crashes the proxy and halts the autonomous OCR process.

#### Implementation Steps:
1. **Inject Context Compression Plugin (`loopmaxxer-bench/escher/proxy.py`):**
   - Locate the `chat_completions` route and the "Mode 1: Direct OpenRouter proxy passthrough" block.
   - Immediately after the `data["provider"]` enforcement logic and before the `is_streaming` check, dynamically inject the `context-compression` plugin into the request data payload.
   - Example modification:
     ```python
     # Enforce zero-data-retention parameters
     data["provider"] = {
         "data_collection": state.get("data_collection", "allow"),
         "zdr": to_bool(state.get("zdr", False)),
         "allow_fallbacks": to_bool(state.get("allow_fallbacks", True)),
         "only": whitelist,
         "require_parameters": to_bool(state.get("require_parameters", False)),
     }

     # Enable automatic prompt compression to prevent 400 Bad Request on large context
     if "plugins" not in data:
         data["plugins"] = []
     if not any(p.get("id") == "context-compression" for p in data["plugins"]):
         data["plugins"].append({"id": "context-compression"})
     ```
]]></description>
  </task>
</tasklist>