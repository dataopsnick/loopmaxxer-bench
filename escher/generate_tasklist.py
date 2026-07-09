# /Users/Nicholas.Cantrell/Workspace/bbbbb/ocr-chat-ui-interface/generate_tasklist.py

import re
import xml.sax.saxutils as saxutils

def escape_cdata(text):
    """
    Escapes the CDATA closing sequence ']]>' to prevent malformed XML structure.
    """
    return text.replace("]]>", "]]]]><![CDATA[>")


def clean_title(path_part, desc_part):
    # Extract file name and line range
    # e.g., "nonuniform-fourier-infill/setup.py:11-16" -> "setup.py:11-16"
    file_ref = path_part.strip().split('/')[-1] if '/' in path_part else path_part.strip()
    
    # Try to extract bold text at the beginning of desc_part
    desc_lines = desc_part.strip().split('\n')
    first_line = desc_lines[0].strip() if desc_lines else ""
    
    bold_match = re.match(r'^\*\*(.*?)\*\*.*', first_line)
    if bold_match:
        short_title = bold_match.group(1).strip()
    else:
        # Fallback to first line up to 60 chars
        short_title = first_line
        if len(short_title) > 60:
            short_title = short_title[:57] + "..."
    
    # Clean up markdown punctuation from short_title
    short_title = short_title.replace('`', '').replace('*', '')
    
    return f"{file_ref} - {short_title}"

def process_review_file(input_file="CODEREVIEW.md", output_file="TASKLIST.md"):
    """
    Reads the review markdown, generates XML task list, saves it to disk,
    and returns the XML string and task count.
    """
    try:
        with open(input_file, "r", encoding="utf-8") as f:
            content = f.read()
    except FileNotFoundError:
        return None, 0

    # Split by the divider
    parts = content.split("───")

    tasks = []
    # Skip index 0 (preamble). Iterate over pairs following it.
    # We need at least 3 parts to form one task (Preamble --- Path --- Desc)
    num_pairs = (len(parts) - 1) // 2
    
    for i in range(1, num_pairs + 1):
        # Ensure indices exist before accessing
        try:
            path_index = 2*i - 1
            desc_index = 2*i
            
            path_part = parts[path_index].strip()
            desc_part = parts[desc_index].strip()
            
            # Basic validation that these parts aren't empty header markers
            if not path_part or not desc_part:
                continue

            title = clean_title(path_part, desc_part)
            
            # Fully transcribe the path part and description part
            full_transcription = f"### Location: {path_part}\n\n{desc_part}"
            
            tasks.append((i, title, full_transcription))
        except IndexError:
            break

    if not tasks:
         return None, 0

    # Now write the XML-style tasklist to TASKLIST.md
    xml_output = []
    xml_output.append("<tasklist>")

    for task_id, title, desc in tasks:
        xml_output.append('  <task status="NOT STARTED">')
        xml_output.append(f"    <id>{task_id}</id>")
        # Escape special XML characters in the title using safer standard library
        escaped_title = saxutils.escape(title)
        xml_output.append(f"    <title>{escaped_title}</title>")
        # Use CDATA for the description to fully preserve all details and code blocks verbatim
        xml_output.append("    <description><![CDATA[")
        # Escape any nested closing CDATA tags safely
        xml_output.append(escape_cdata(desc))
        xml_output.append("]]></description>")
        xml_output.append("  </task>")

    xml_output.append("</tasklist>")

    final_xml_content = "\n".join(xml_output) + "\n"

    with open(output_file, "w", encoding="utf-8") as f:
        f.write(final_xml_content)
    
    return final_xml_content, len(tasks)


if __name__ == "__main__":
    # Keep existing behavior when run as a script
    xml_content, count = process_review_file()
    if count > 0:
        print(f"Successfully generated TASKLIST.md with {count} tasks.")
    else:
        print("No tasks generated or CODEREVIEW.md not found.")