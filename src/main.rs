// email-learn: store (draft, final) email pairs + agent-derived voice lessons in SQLite.
// ponytail: no LLM API call from the CLI on purpose — the agent reads diffs and
// derives lessons in-session via the email-voice skill. Upgrade path: add a
// `derive` subcommand later that shells out to a provider if you want offline runs.

use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};
use serde::Serialize;
use similar::{ChangeTag, TextDiff};
use std::path::PathBuf;
use std::process::ExitCode;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS pairs (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    draft        TEXT NOT NULL,
    final        TEXT NOT NULL,
    diff         TEXT NOT NULL,
    context      TEXT,
    tags         TEXT,
    created_at   TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS lessons (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    pair_id      INTEGER REFERENCES pairs(id) ON DELETE SET NULL,
    lesson       TEXT NOT NULL,
    tags         TEXT,
    created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pairs_tags ON pairs(tags);
CREATE INDEX IF NOT EXISTS idx_lessons_tags ON lessons(tags);
";

fn db_path() -> PathBuf {
    // ponytail: env override for tests / CI; default to repo-local emails.db.
    if let Ok(p) = std::env::var("EMAIL_LEARN_DB") {
        return PathBuf::from(p);
    }
    PathBuf::from("emails.db")
}

fn connect() -> anyhow::Result<Connection> {
    let conn = Connection::open(db_path())?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Unified-diff-style text. ponytail: line-based; char-level granularity not needed
/// for prose — upgrade to `diff::chars` if you start diffing one-liners.
fn unified_diff(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => '-',
            ChangeTag::Insert => '+',
            ChangeTag::Equal => ' ',
        };
        let text = change.value();
        // trim a trailing newline so signs line up with content
        out.push(sign);
        out.push_str(text);
        if !text.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

#[derive(Parser)]
#[command(name = "email-learn", version, about = "Store (draft, final) email pairs + voice lessons for agent learning.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Store a new (draft, final) email pair. Prints the new pair id.
    Add {
        draft_path: PathBuf,
        final_path: PathBuf,
        /// Free-form context (topic, recipient type, intent).
        #[arg(long)]
        context: Option<String>,
        /// Comma-separated tags.
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Show one pair (draft, final, diff) for in-session lesson derivation.
    Show { id: i64 },
    /// List the N most recent pairs (id, created_at, context, tags, diff).
    #[command(alias = "ls")]
    Recent {
        #[arg(default_value = "10")]
        n: usize,
    },
    /// List stored voice lessons.
    Lessons {
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Store a lesson derived from a pair by the agent.
    AddLesson {
        pair_id: i64,
        lesson: String,
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
    },
    /// Full-text-ish search across pairs (context, tags, final body) and lessons.
    Query { needle: String },
    /// Dump everything as markdown for bulk injection into an agent prompt.
    Export,
}

#[derive(Serialize)]
struct Pair {
    id: i64,
    draft: String,
    final_: String,
    diff: String,
    context: Option<String>,
    tags: Vec<String>,
    created_at: String,
}

#[derive(Serialize)]
struct Lesson {
    id: i64,
    pair_id: Option<i64>,
    lesson: String,
    tags: Vec<String>,
    created_at: String,
}

fn parse_tags(s: Option<&str>) -> Vec<String> {
    match s {
        None | Some("") => Vec::new(),
        Some(t) => serde_json::from_str::<Vec<String>>(t).unwrap_or_default(),
    }
}

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let conn = connect()?;
    match cli.cmd {
        Cmd::Add { draft_path, final_path, context, tags } => {
            let draft = std::fs::read_to_string(&draft_path)
                .map_err(|e| anyhow::anyhow!("read draft {}: {e}", draft_path.display()))?;
            let final_ = std::fs::read_to_string(&final_path)
                .map_err(|e| anyhow::anyhow!("read final {}: {e}", final_path.display()))?;
            let diff = unified_diff(&draft, &final_);
            let tags_json = tags_to_json(&tags);
            let now = now_iso();
            conn.execute(
                "INSERT INTO pairs (draft, final, diff, context, tags, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![draft, final_, diff, context, tags_json, now],
            )?;
            let id = conn.last_insert_rowid();
            println!("{id}");
            Ok(())
        }
        Cmd::Show { id } => {
            let mut stmt = conn.prepare("SELECT id, draft, final, diff, context, tags, created_at FROM pairs WHERE id = ?1")?;
            let mut rows = stmt.query(params![id])?;
            if let Some(r) = rows.next()? {
                let tags: Vec<String> = parse_tags(r.get::<_, Option<String>>(5)?.as_deref());
                let p = Pair {
                    id: r.get(0)?,
                    draft: r.get(1)?,
                    final_: r.get(2)?,
                    diff: r.get(3)?,
                    context: r.get(4)?,
                    tags,
                    created_at: r.get(6)?,
                };
                println!("{}", serde_json::to_string_pretty(&p)?);
            } else {
                eprintln!("no pair with id {id}");
            }
            Ok(())
        }
        Cmd::Recent { n } => {
            let mut stmt = conn.prepare("SELECT id, draft, final, diff, context, tags, created_at FROM pairs ORDER BY id DESC LIMIT ?1")?;
            let mut out = Vec::new();
            let rows = stmt.query_map(params![n as i64], |r| {
                let tags: Vec<String> = parse_tags(r.get::<_, Option<String>>(5)?.as_deref());
                Ok(Pair {
                    id: r.get(0)?,
                    draft: r.get(1)?,
                    final_: r.get(2)?,
                    diff: r.get(3)?,
                    context: r.get(4)?,
                    tags,
                    created_at: r.get(6)?,
                })
            })?;
            for r in rows { out.push(r?); }
            println!("{}", serde_json::to_string_pretty(&out)?);
            Ok(())
        }
        Cmd::Lessons { tags } => {
            // ponytail: per-tag LIKE (any-match). Upgrade to FTS5 / json_each if scale demands.
            let mut out: Vec<Lesson> = Vec::new();
            if tags.is_empty() {
                let mut stmt = conn.prepare("SELECT id, pair_id, lesson, tags, created_at FROM lessons ORDER BY id DESC")?;
                let rows = stmt.query_map([], |r| {
                    let t: Vec<String> = parse_tags(r.get::<_, Option<String>>(3)?.as_deref());
                    Ok(Lesson { id: r.get(0)?, pair_id: r.get(1)?, lesson: r.get(2)?, tags: t, created_at: r.get(4)? })
                })?;
                for x in rows { out.push(x?); }
            } else {
                let pats: Vec<String> = tags.iter().map(|t| format!("%\"{}\"%", t.replace('"', "\\\""))).collect();
                let placeholders = (0..pats.len()).map(|_| "tags LIKE ?").collect::<Vec<_>>().join(" OR ");
                let sql = format!("SELECT id, pair_id, lesson, tags, created_at FROM lessons WHERE {placeholders} ORDER BY id DESC");
                let mut stmt = conn.prepare(&sql)?;
                let refs: Vec<&dyn rusqlite::ToSql> = pats.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
                let rows = stmt.query_map(refs.as_slice(), |r| {
                    let t: Vec<String> = parse_tags(r.get::<_, Option<String>>(3)?.as_deref());
                    Ok(Lesson { id: r.get(0)?, pair_id: r.get(1)?, lesson: r.get(2)?, tags: t, created_at: r.get(4)? })
                })?;
                for x in rows { out.push(x?); }
            }
            let s = serde_json::to_string_pretty(&out)?;
            println!("{s}");
            Ok(())
        }
        Cmd::AddLesson { pair_id, lesson, tags } => {
            let tags_json = tags_to_json(&tags);
            let now = now_iso();
            conn.execute(
                "INSERT INTO lessons (pair_id, lesson, tags, created_at) VALUES (?1,?2,?3,?4)",
                params![pair_id, lesson, tags_json, now],
            )?;
            println!("{}", conn.last_insert_rowid());
            Ok(())
        }
        Cmd::Query { needle } => {
            let pat = format!("%{needle}%");
            let mut pairs: Vec<Pair> = Vec::new();
            {
                let mut stmt = conn.prepare(
                    "SELECT id, draft, final, diff, context, tags, created_at FROM pairs
                     WHERE context LIKE ?1 OR tags LIKE ?1 OR final LIKE ?1 OR draft LIKE ?1
                     ORDER BY id DESC LIMIT 50"
                )?;
                let rows = stmt.query_map(params![pat], |r| {
                    let tags: Vec<String> = parse_tags(r.get::<_, Option<String>>(5)?.as_deref());
                    Ok(Pair { id: r.get(0)?, draft: r.get(1)?, final_: r.get(2)?, diff: r.get(3)?, context: r.get(4)?, tags, created_at: r.get(6)? })
                })?;
                for x in rows { pairs.push(x?); }
            }
            let mut lessons: Vec<Lesson> = Vec::new();
            {
                let mut stmt = conn.prepare(
                    "SELECT id, pair_id, lesson, tags, created_at FROM lessons
                     WHERE lesson LIKE ?1 OR tags LIKE ?1 ORDER BY id DESC LIMIT 50"
                )?;
                let rows = stmt.query_map(params![pat], |r| {
                    let t: Vec<String> = parse_tags(r.get::<_, Option<String>>(3)?.as_deref());
                    Ok(Lesson { id: r.get(0)?, pair_id: r.get(1)?, lesson: r.get(2)?, tags: t, created_at: r.get(4)? })
                })?;
                for x in rows { lessons.push(x?); }
            }
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "pairs": pairs,
                "lessons": lessons,
            }))?);
            Ok(())
        }
        Cmd::Export => {
            let mut pairs: Vec<Pair> = Vec::new();
            {
                let mut stmt = conn.prepare("SELECT id, draft, final, diff, context, tags, created_at FROM pairs ORDER BY id ASC")?;
                let rows = stmt.query_map([], |r| {
                    let tags: Vec<String> = parse_tags(r.get::<_, Option<String>>(5)?.as_deref());
                    Ok(Pair { id: r.get(0)?, draft: r.get(1)?, final_: r.get(2)?, diff: r.get(3)?, context: r.get(4)?, tags, created_at: r.get(6)? })
                })?;
                for x in rows { pairs.push(x?); }
            }
            let mut lessons: Vec<Lesson> = Vec::new();
            {
                let mut stmt = conn.prepare("SELECT id, pair_id, lesson, tags, created_at FROM lessons ORDER BY id ASC")?;
                let rows = stmt.query_map([], |r| {
                    let t: Vec<String> = parse_tags(r.get::<_, Option<String>>(3)?.as_deref());
                    Ok(Lesson { id: r.get(0)?, pair_id: r.get(1)?, lesson: r.get(2)?, tags: t, created_at: r.get(4)? })
                })?;
                for x in rows { lessons.push(x?); }
            }

            let mut md = String::new();
            md.push_str("# Voice Lessons (exported)\n\n");
            md.push_str("## Lessons\n\n");
            if lessons.is_empty() {
                md.push_str("_(none yet)_\n\n");
            }
            for l in &lessons {
                md.push_str(&format!("- **L{}** (pair #{}) {}: {}  _{}_\n",
                    l.id, l.pair_id.map(|i| i.to_string()).unwrap_or_else(|| "—".into()),
                    l.tags.join(","), l.lesson, l.created_at));
            }
            md.push_str("\n## Pairs\n\n");
            for p in &pairs {
                md.push_str(&format!("### Pair #{} — {}\n", p.id, p.created_at));
                if let Some(c) = &p.context { md.push_str(&format!("context: {c}\n")); }
                if !p.tags.is_empty() { md.push_str(&format!("tags: {}\n", p.tags.join(", "))); }
                md.push_str("\n#### Draft\n```\n");
                md.push_str(&p.draft);
                if !p.draft.ends_with('\n') { md.push('\n'); }
                md.push_str("```\n#### Final\n```\n");
                md.push_str(&p.final_);
                if !p.final_.ends_with('\n') { md.push('\n'); }
                md.push_str("```\n#### Diff\n```diff\n");
                md.push_str(&p.diff);
                if !p.diff.ends_with('\n') { md.push('\n'); }
                md.push_str("```\n\n");
            }
            println!("{md}");
            Ok(())
        }
    }
}
