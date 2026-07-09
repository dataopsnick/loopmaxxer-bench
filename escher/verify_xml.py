import xml.etree.ElementTree as ET

try:
    tree = ET.parse("TASKLIST.md")
    print("SUCCESS: TASKLIST.md is well-formed XML!")
except Exception as e:
    print(f"ERROR: XML parsing failed: {e}")
