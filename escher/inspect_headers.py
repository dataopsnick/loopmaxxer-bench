import os
import sys
import re

filename = "CODEREVIEW.md"
if not os.path.exists(filename):
    print(f"Error: {filename} not found.")
    sys.exit(1)

with open(filename, "r", encoding="utf-8") as f:
    content = f.read()

matches = list(re.finditer(r'───\s*(.*?)\s*───', content))
print(f"Found {len(matches)} headers:")
for idx, match in enumerate(matches):
    print(f"{idx+1}: {match.group(1)} at position {match.start()}")
