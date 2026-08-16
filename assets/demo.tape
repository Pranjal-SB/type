# A VHS tape — https://github.com/charmbracelet/vhs
#
# Renders assets/demo.gif deterministically, so the README's demo is
# regenerated per release rather than re-recorded by hand:
#
#     cargo build --release
#     vhs assets/demo.tape
#
# Keep it under about twenty seconds. A README GIF is watched once, at most.

Output assets/demo.gif

Set Shell bash
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 20
Set TypingSpeed 60ms

Hide
Type "cd $(mktemp -d) && git init -q . && mkdir -p src" Enter
Type "printf 'fn main() {\n    let editor = Editor::new();\n    editor.run()\n}\n' > src/main.rs" Enter
Type "printf '[package]\nname = \"demo\"\n' > Cargo.toml" Enter
Type "clear" Enter
Show

# Cold start into a project — the file tree and the editor, side by side.
Type "typ ." Sleep 2s Enter
Sleep 2s

# The tree: move down to src/, expand it, open the file.
Down 1 Sleep 500ms
Right Sleep 700ms
Down 1 Sleep 500ms
Enter Sleep 1.5s

# Focus the editor and type. The gutter, the theme and the current-line
# highlight are all doing their work in the background here.
F6 Sleep 800ms
Down 1 Sleep 300ms
End Sleep 500ms
Enter
Type "    editor.run();" Sleep 1s

# Multi-cursor by occurrence — the thing people mean by multi-cursor.
Ctrl+Home Sleep 500ms
Ctrl+Right Sleep 400ms
Ctrl+D Sleep 800ms
Ctrl+D Sleep 800ms
Ctrl+D Sleep 1s

# One edit, every cursor.
Type "buffer" Sleep 1.5s
Escape Sleep 500ms

# Undo takes back the whole run, not one character at a time.
Ctrl+Z Sleep 1.5s

# Search, smart-case.
Ctrl+F Sleep 500ms
Type "Editor" Sleep 800ms
Enter Sleep 1.5s

Ctrl+S Sleep 1s
Ctrl+Q Sleep 1.5s
