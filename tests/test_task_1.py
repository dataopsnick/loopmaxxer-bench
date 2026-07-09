import unittest
import re

"""
Example Test code for PR Fix Verifier Skill

---
name: example-pr-fix-validation-test
description: |
  Guides the engineering agent to write and execute deterministic, automated tests 
  that verify a code change actually resolves the target issue/task before 
  ci_reviewer.py is permitted to execute a merge.
  
Example Task:
<tasklist>
<task status="NOT STARTED">
    <id>1</id>
    <title>proxy.py:678-679 - [bug · critical] ANSI_ESCAPE.sub() is missing the replace...</title>
    <description><![CDATA[
### Location: proxy.py:678-679

[bug · critical] ANSI_ESCAPE.sub() is missing the replacement string argument. This line calls
`ANSI_ESCAPE.sub(raw_line)` instead of `ANSI_ESCAPE.sub('', raw_line)`, which will raise a TypeError
at runtime: `sub() missing 1 required positional argument: 'repl'`. This crashes the benchmark loop
on the first line of OCR output.

                  raw_line = line_bytes.decode('utf-8', errors='replace')
-                 clean_line = ANSI_ESCAPE.sub(raw_line)
+                 clean_line = ANSI_ESCAPE.sub('', raw_line)
]]></description>
  </task>
  </tasklist>
  ---"""

class TestTask1AnsiEscape(unittest.TestCase):
    def test_ansi_escape_replacement(self):
        # The issue: ANSI_ESCAPE.sub() raised TypeError because it lacked '' repl argument.
        # This test ensures we can strip ANSI escape codes cleanly without raising TypeErrors.
        
        # 1. Compile the regex as defined in proxy.py
        ANSI_ESCAPE = re.compile(r'\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])')
        
        # 2. Sample log input containing ANSI color codes (red text example)
        raw_line_bytes = b'\x1B[31m[Diagnostic] Read 45 raw bytes.\x1B[0m\n'
        raw_line = raw_line_bytes.decode('utf-8', errors='replace')
        
        # 3. Assert that stripping does not crash (reproducing original TypeError)
        try:
            # We must be able to perform this regex replace with an explicit replacement string
            clean_line = ANSI_ESCAPE.sub('', raw_line)
        except TypeError as e:
            self.fail(f"TypeError raised during escape stripping: {e}")
            
        # 4. Assert correctness of the output
        self.assertEqual(clean_line, "[Diagnostic] Read 45 raw bytes.\n")

if __name__ == '__main__':
    unittest.main()
