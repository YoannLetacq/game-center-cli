---
name: teammate
description: Spawn a teammate agent to assist with a specific task. Use proactively whenever you need help — research, code review, exploration, testing, or parallel work. Invoke manually with /teammate or automatically when facing complex multi-part work.
user-invocable: true
disable-model-invocation: false
allowed-tools: Agent(*) Bash(cargo *) Bash(git *) Read(*) Grep(*) Glob(*)
argument-hint: [task-description]
---

# Teammate — Spawn a Helper Agent

You have been asked to create a teammate to help with: **$ARGUMENTS**

## How to Use

Analyze the task and spawn the right type of agent using the Agent tool.

### Agent Type Selection

Pick the agent type based on the task:

| Task | Agent Type |
|------|-----------|
| Find files, search code, understand structure | `Explore` |
| Design an approach, plan implementation | `Plan` |
| Implement code, fix bugs, run commands | `general-purpose` |
| Questions about Claude Code features | `claude-code-guide` |

### Spawn Rules

1. **Be specific in the prompt**: The teammate has zero context. Include file paths, what you've tried, what you need, and why.
2. **Name the agent** descriptively so you can send follow-up messages via `SendMessage(to: "name")`.
3. **Use background mode** when you have other work to do in parallel. Use foreground when you need the result before continuing.
4. **Parallel teammates**: If the task has independent parts, spawn multiple agents in a single message for maximum efficiency.
5. **Never delegate understanding**: Don't ask the teammate to "figure it out and fix it." Tell it what to look at and what outcome you expect.

### Examples

**Research teammate** (background):
```
Agent({
  name: "research-auth",
  subagent_type: "Explore",
  prompt: "Find all authentication-related code in crates/server/src/auth/. Map out the JWT flow from token creation to validation. Report file paths and key function signatures.",
  run_in_background: true
})
```

**Implementation teammate** (foreground):
```
Agent({
  name: "impl-connect4",
  subagent_type: "general-purpose",
  prompt: "Implement the Connect 4 game engine in crates/shared/src/game/connect4.rs following the GameEngine trait defined in crates/shared/src/game/traits.rs. Include validate_move, apply_move, is_terminal with win detection (horizontal, vertical, diagonal). Add a bot_move function with Easy (random) and Hard (alpha-beta depth 6) difficulties. Write comprehensive tests.",
  mode: "auto"
})
```

**Parallel teammates** (multiple at once):
```
Agent({ name: "test-writer", subagent_type: "general-purpose", prompt: "...", run_in_background: true })
Agent({ name: "doc-reviewer", subagent_type: "Explore", prompt: "...", run_in_background: true })
```

## When to Spawn Automatically

Use this skill proactively (without being asked) when:
- You're about to do a task with 3+ independent parts — parallelize with teammates
- You need to research something without polluting your main context
- You're implementing a feature and want a second pair of eyes on a related file
- A test fails and you want help investigating while you continue other work
- You need to explore an unfamiliar part of the codebase

## After the Teammate Returns

- Summarize the teammate's findings to the user concisely
- If the teammate produced code, review it before presenting — you own the quality
- If follow-up is needed, use `SendMessage(to: "agent-name")` to continue the conversation
