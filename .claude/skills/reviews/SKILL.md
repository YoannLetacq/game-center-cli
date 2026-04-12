---
name: reviews
description: Create a code review request for an external reviewer before feature release. Generates discussion/REVIEW.md with context for review and reads back discussion/FEEDBACK.md if it exists.
user-invocable: true
disable-model-invocation: true
allowed-tools: Bash(git *) Bash(mkdir *) Edit(*) Write(*) Read(*)
argument-hint: [feature-name]
---

# External Code Review — $ARGUMENTS

Prepare a review package for external review of the feature **$ARGUMENTS**.

## Steps

1. **Create the discussion folder**: `mkdir -p discussion/`

2. **Check for existing feedback**: Read `discussion/FEEDBACK.md` if it exists. If it contains prior feedback, summarize what was addressed and what remains.

3. **Gather context**:
   - Run `git diff main...HEAD --stat` to list all changed files.
   - Run `git log main...HEAD --oneline` to list all commits for this feature.
   - Read the key files that were changed to understand the implementation.

4. **Write `discussion/REVIEW.md`** with this structure:

```markdown
# Code Review Request: [Feature Name]

## Summary
Brief description of what this feature does and why.

## Changes Overview
List of files changed with a one-line description of each change.

## Architecture Decisions
Key design choices made and their rationale.

## Areas of Concern
Specific areas where reviewer input is valuable:
- Performance considerations
- Security implications
- Edge cases that may not be covered
- API design choices

## How to Test
Steps to verify the feature works correctly.

## Questions for Reviewer
Specific questions for the reviewer to address.
```

5. **Notify**: Tell the user that `discussion/REVIEW.md` is ready. Remind them to place feedback in `discussion/FEEDBACK.md` for follow-up.

## On Follow-Up

If `discussion/FEEDBACK.md` already exists when this skill is invoked, read it first, summarize the feedback, and update `discussion/REVIEW.md` to reflect what has been addressed since last review.
