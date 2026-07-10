import os

filename = "CODEREVIEW.md"
if os.path.exists(filename):
    with open(filename, "r", encoding="utf-8") as f:
        content = f.read()
else:
    content = ""

parts = content.split("───")
print(f"Total parts: {len(parts)}")

if len(parts) <= 1:
    print("Warning: No '───' separators found in CODEREVIEW.md")
for i in range(1, len(parts)):
    stripped_part = parts[i].strip()
    if not stripped_part:
        print(f"{i}: [Empty Part]")
        continue
    header = stripped_part.split("\n")[0].strip()
    print(f"{i}: {header[:60]}...")
