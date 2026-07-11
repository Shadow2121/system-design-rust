---
name: dream
description: Memory consolidation system. Run when pending.txt is found or manually triggered.
---

# Dream Job Instructions

You are running the automated memory consolidation job ("dream").
Your goal is to process the conversation transcripts staged in `.agents/transcripts/`, extract key insights, and update the project's memory.

## 1. Check Staging
Look for `.agents/transcripts/pending.txt`. If it doesn't exist, say "No new memories to process" and stop.
Read `pending.txt`. It contains a header and a list of `delta_<conv-id>.jsonl` files.

## 2. Process Transcripts (Batching limit: 3)
Pick the first 3 `.jsonl` files listed in `pending.txt`. If there are more than 3, ONLY process the first 3 this time to protect your context window.
For each of the 3 files:
1. Read the file (`.agents/transcripts/<filename>`).
2. Extract the following structured data:
   - **DECISIONS**: Architectural choices ("we chose X over Y because...").
   - **CODE**: Files created/modified, functions added, crates changed.
   - **PROBLEMS SOLVED**: Errors fixed, bugs resolved, compile failures.
   - **PATTERNS**: Conventions established, preferred approaches.
   - **CRATE STATUS**: Any status changes (started / in-progress / complete).
   - **OPEN QUESTIONS**: Unresolved threads.

## 3. Update Knowledge Items (Idempotent)
Write or update files in `.agents/knowledge/<topic>.md`.
Each KI file must have two sections: `## ⚡ Current State` and `## 📖 History`.
- Rewrite `## ⚡ Current State` with the latest summary.
- Append to `## 📖 History` using the format: `### Update from transcript <conv-id>`.
  - **CRITICAL**: Before appending, check if the exact tag `### Update from transcript <conv-id>` already exists in the file. If it does, DO NOT append it again. This makes the update idempotent.
- Do NOT overwrite any block starting with `<!-- MANUAL OVERRIDE -->`.

## 4. Update Second Brain
- Write a summary of what was learned to `.agents/brain/daily/<YYYY-MM-DD>.md`.
- Create `.agents/brain/concepts/<topic>.md` if a new concept was explained.
- Create `.agents/brain/decisions/<topic>.md` if an architectural decision was made.
### Tone and Phrasing Guidelines for the Second Brain:
- **Write in the First Person:** Phrase everything as if the human user is writing it to their future self (e.g., "Today I realized...", "We decided to...").
- **Reflective & Insightful:** Do not just list facts. Focus heavily on the *why*. Why did we make this architectural decision? What was the "aha!" moment?
- **Avoid AI-Speak:** Strip out all robotic transitions ("In conclusion", "As an AI assistant", "Here is a summary"). Make it read like a senior engineer's personal journal.
- **Use Analogies:** When creating concept notes, try to anchor highly technical distributed systems concepts to simple real-world analogies. 
- **Formatting:** Liberally use **bolding** for key terms, short punchy bullet points, and `[[wikilinks]]` to organically connect thoughts.

## 5. Clean Up
- Update `.agents/knowledge/INDEX.md` with the latest summary of the KIs.
- Delete the `.jsonl` files you just processed.
- Remove the processed files from `pending.txt`. If `pending.txt` is now empty (only header remaining), delete `pending.txt`.
