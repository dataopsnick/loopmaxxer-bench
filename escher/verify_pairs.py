with open("CODEREVIEW.md", "r", encoding="utf-8") as f:
    content = f.read()

parts = content.split("───")
print(f"Total parts: {len(parts)}")

for i in range(1, len(parts)):
    header = parts[i].strip().split("\n")[0].strip()
    print(f"{i}: {header[:60]}...")
