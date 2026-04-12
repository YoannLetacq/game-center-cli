---
name: epic-compact
description: Custom conversation compaction. Exports the full conversation to a text file and creates SUMMARY.md with key discussion points and references. Use when context is getting large or before a natural breakpoint.
user-invocable: true
disable-model-invocation: false
allowed-tools: Bash(mkdir *) Bash(date *) Bash(wc *) Bash(ls *) Edit(*) Write(*) Read(*)
---

# Epic Compact — Conversation Preservation

Preserve the current conversation context before it gets compacted or cleared.

## Steps

1. **Create export directory**: `mkdir -p .claude/exports/`

2. **Generate timestamp**: Use format `YYYY-MM-DD_HH-MM` for filenames.

3. **Export conversation**: Use `/export` to export the current conversation to `.claude/exports/conversation_<timestamp>.txt`.

4. **Create SUMMARY.md**: Write `.claude/exports/SUMMARY.md` (overwrite if exists) with this structure:

```markdown
# Conversation History Index

## Latest Session: <timestamp>
Export file: `.claude/exports/conversation_<timestamp>.txt`

### Key Decisions
- [Decision 1]: Brief description — see export line ~N
- [Decision 2]: Brief description — see export line ~N

### Implementation Progress
- [x] Completed item with brief note
- [ ] Pending item with brief note

### Architecture Notes
Key architectural decisions and patterns established.

### Active Context
- Current branch: <branch>
- Current phase: <what we're working on>
- Next steps: <what comes next>

### Important References
- File paths, line numbers, or code patterns that were discussed
- Any external resources or documentation referenced

---

## Previous Sessions
<Append previous session entries here, keep last 5>
```

5. **Read previous SUMMARY.md** if it exists: Preserve the "Previous Sessions" section by appending the old "Latest Session" entry to it. Keep only the last 5 session entries.

6. **Confirm**: Tell the user the export is saved and they can safely clear context. After clearing, remind them to read `.claude/exports/SUMMARY.md` to restore context.

## On Session Start (After Clear)

If the user mentions restoring context or references a previous session:
1. Read `.claude/exports/SUMMARY.md` for the index.
2. Based on what's needed, read specific sections of the exported conversation file.
3. Summarize the relevant context concisely.
