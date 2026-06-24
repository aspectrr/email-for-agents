# email-for-agents

A small Rust CLI + agent skill that lets coding agents (pi, Claude Code, Cursor, …) write emails in your voice by learning from **(draft → final)** revision pairs you make.

The CLI just stores pairs and lessons in SQLite and surfaces them. All the *reasoning* (deriving voice rules from diffs) happens in the agent session via the bundled `email-voice` skill — no LLM call leaves your agent.

## Install

```bash
cargo install --path .
```

Puts the `email-learn` binary on your `PATH` (in `~/.cargo/bin`).

## Use it as an agent skill

The skill lives in `skills/email-voice/SKILL.md`. Symlink it into your agent's global skills directory:

```bash
# pi
ln -s "$PWD/skills/email-voice" ~/.pi/agent/skills/email-voice
```

Then any pi agent can load it by name (`email-voice`) and follow its workflows.

## Data location

- DB defaults to `~/.email-learn/emails.db` — global, shared across all projects so your voice lessons accumulate everywhere.
- Override with `EMAIL_LEARN_DB=/abs/path/emails.db`.

## Commands

```
email-learn add <draft> <final> --context "<one line>" --tags a,b      # store a pair → prints id
email-learn show <id>                                                  # draft + final + diff as JSON
email-learn recent [N]                                                 # N most recent pairs (default 10)
email-learn lessons [--tags a,b]                                       # stored voice lessons
email-learn add-lesson <pair_id> "<lesson>" --tags a,b                 # record a derived rule
email-learn query "<needle>"                                           # LIKE search pairs + lessons
email-learn export                                                     # everything as one markdown dump
```

## How learning works

1. You give the agent a draft you wrote **and** the version you actually sent.
2. The agent stores the pair, reads the diff, and derives 1–3 specific voice lessons
   (word swaps, structural moves, tone shifts, things you *never* do).
3. Next time you ask the agent to draft an email, it pulls `lessons`, `query`, and `recent`
   first and writes to your rules.

See `skills/email-voice/SKILL.md` for the full workflow and what counts as a good lesson.

## License

MIT
