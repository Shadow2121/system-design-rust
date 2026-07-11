# Dream Memory System

## ⚡ Current State
The project utilizes an automated "Dream" memory consolidation system to retain cross-session context.
Architecture:
- **PowerShell Bridge**: A scheduled task runs `dream_master.ps1` every 2 hours/midnight. It bypasses the global `.gemini/` permission wall, extracts new conversation steps, filters by the workspace path (via active document fingerprint), and stages delta files in `.agents/transcripts/`.
- **Agent Skill**: Triggered automatically on new conversations by `AGENTS.md`. Parses staged files, extracts decisions, and writes to `.agents/knowledge/` (agent optimized) and `.agents/brain/` (human-readable Obsidian vault).
- **Idempotency**: All history writes use tagging (`### Update from transcript <conv-id>`) to prevent duplicate entries if the agent fails midway.

## 📖 History
### Update from transcript 1400a764-7e5b-4660-a54a-393596d48641
- Conceived the automated memory consolidation concept.
- Shifted from a global /schedule command to a PowerShell Task Scheduler bridge to solve the system protection boundary read issue.
- Designed the split between `.agents/knowledge/` for KIs and `.agents/brain/` for an Obsidian-compatible second brain.
- Finalized true incremental processing and step-tracking to safely process active conversations.
