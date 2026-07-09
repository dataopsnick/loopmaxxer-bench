import re

with open("CODEREVIEW.md", "r", encoding="utf-8") as f:
    content = f.read()

# Let's find all headers starting with ───
matches = list(re.finditer(r'───\s*(.*?)\s*───', content))
print(f"Found {len(matches)} headers:")
for idx, match in enumerate(matches):
    print(f"{idx+1}: {match.group(1)} at position {match.start()}")
