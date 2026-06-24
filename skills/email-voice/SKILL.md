---
name: email-voice
description: Write emails that sound like the user by learning from (draft → final) revisions. Use when drafting, revising, or reviewing emails; and whenever the user shares a draft alongside the version they actually sent. Stores pairs and lessons in a local SQLite db via the `email-learn` Rust CLI.
---

# email-voice

Make outbound emails sound like the user. The system learns from **(draft, final)** revision pairs — first version vs. the version actually sent. You derive the insight in-session; the CLI just stores and retrieves.

## Where the data lives

- DB: `~/.email-learn/emails.db` (global, shared across all projects). Override with `EMAIL_LEARN_DB=/path/db`.
- CLI binary: `email-learn` (built from this repo — run `cargo build --release` first, then `./target/release/email-learn`, or alias it)

The CLI does **not** call any LLM. You do all reasoning here, against the diffs it produces.

## Workflow A — writing a new email

Before drafting:

1. `email-learn lessons` — read every stored voice lesson. These are the rules.
2. `email-learn query "<topic or recipient>"` — pull any prior pairs whose context/tags/body match.
3. `email-learn recent 5` — skim the 5 most recent finals to calibrate current voice.

Then write the draft applying the lessons. Cite the lesson id inline only if the user asks; otherwise just write it.

## Workflow B — learning from a revision

When the user gives you a draft **and** the version they actually sent:

1. Save both to files (or pipe via stdin — but files are simplest).
2. `email-learn add draft.txt final.txt --context "<one line: topic + recipient type>" --tags pitch,external` → prints the new pair id.
3. `email-learn show <id>` — read the diff (lines marked ` ` equal, `-` removed from draft, `+` added in final).
4. **Derive 1–3 concrete lessons** from the diff. A good lesson is:
   - **Specific**, not generic. "Cut the opening pleasantry from 3 sentences to 1" beats "be concise."
   - **Actionable** — names a swap or structural move the agent can repeat.
   - **Voice-coded** — about *how Collin writes*, not about correctness.
5. `email-learn add-lesson <id> "<the lesson>" --tags <relevant>` for each.

### What counts as a lesson

- Word/phrase swaps the user consistently makes (`utilize → use`, `I wanted to reach out → quick note`).
- Structural patterns (single ask per email, no nested bullets, sign-off choice).
- Tone shifts (more direct, less hedging, fewer apologies).
- Length targets (target N sentences for this category).
- Things the user **never** does (negative lessons are gold: "never open with 'I hope this finds you well'").

Bad lesson: "Be clear and professional." Useless. Reject your own draft if it sounds like that.

### Deriving from a single pair

Don't over-fit. If a swap appears once, note it as a *candidate*, tag it `unconfirmed`. Only promote to a strong rule once you've seen it 2–3 times across pairs. Before writing a new lesson, `email-learn query "<the pattern>"` to see if it's already captured.

## Workflow C — bulk review / refresh

When the user asks to "refresh the voice model" or similar:

1. `email-learn export` → markdown dump of all pairs + lessons.
2. Read end-to-end. Identify duplicates, contradictions, weak lessons.
3. Propose edits: tell the user which lesson ids to delete and what to add. **Do not delete silently** — the CLI has no delete yet and lessons are the user's voice record. Ask.

## Conventions

- Tags: lowercase, comma-separated, no spaces. Suggested vocab: `pitch`, `followup`, `external`, `internal`, `apology`, `decline`, `intro`, `unconfirmed`.
- Context line: recipient type + intent, e.g. `"cold intro to investor"` or `"internal status update"`.
- One ask per email is a near-universal rule — default to it unless a pair teaches otherwise.

## Failure modes to watch

- **Over-fitting to one pair.** Confirm across ≥2 before promoting.
- **Generic lessons.** If you can't say *what specifically changed*, don't store a lesson.
- **Stale voice.** Re-check `recent` periodically; voice drifts. If a final contradicts an old lesson, flag it.
- **Silent edits.** Never modify or delete lessons without surfacing the change to the user.
