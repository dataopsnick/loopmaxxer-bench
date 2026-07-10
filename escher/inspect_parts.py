import os

filename = "CODEREVIEW.md"
if os.path.exists(filename):
    with open(filename, "r", encoding="utf-8") as f:
        content = f.read()
else:
    content = ""

parts = content.split("───") if content else []
print(f"Total parts: {len(parts)}")
for i in range(1, len(parts)):
    lines = parts[i].strip().split("\n")
    if lines:
        header = lines[0].strip()
        body_sample = "\n".join(lines[1:5])
        print(f"\nPart {i}: Header = '{header}'")
        print(f"Body Sample:\n{body_sample}")
