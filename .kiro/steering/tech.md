---
inclusion: always
---

# Technology Stack

Prefer the choices in this document over alternatives when suggesting or generating code. Where a choice is marked **provisional**, it can change, but not silently: raise it first.

## Language and toolchain

- **Language:** Rust, stable toolchain. Pin a minimum supported Rust version (MSRV) in `Cargo.toml` and CI.
- **Build:** Cargo workspace (see `structure.md`).
- **Formatting and linting:** `rustfmt` and `clippy`, with warnings denied in CI.
- **Unsafe code:** avoid. If it is ever needed, isolate it and document why.
- **Error handling:** no `unwrap` or `expect` outside tests and provably infallible spots. Library crates use typed errors (`thiserror`). The binary layer reports errors with context and readable diagnostics (`miette` or `anyhow`).

## Core dependencies

| Purpose | Choice | Notes |
|---|---|---|
| Parsing | `tree-sitter` with `tree-sitter-python` | Incremental, error-tolerant. Grammars are crates. |
| Storage | `rusqlite` with the `bundled` feature | Bundled SQLite gives identical behavior on every platform. |
| CLI | `clap` (derive) | Kebab-case flags, generated help and completions. |
| Serialization | `serde`, `serde_json` | JSON output and internal data. |
| Config | `toml` crate, `serde`, `schemars` | The schema comes from Rust types and is published as JSON schema. |
| File walking | `ignore` | Respects `.gitignore` and similar rules. |
| Parallelism | `rayon` | Parallel per-file extraction. |
| Hashing | `blake3` | Content hashes for change detection. |
| Testing | `insta`, `assert_cmd`, `tempfile` | Snapshot tests and CLI tests. |
| Benchmarks | `criterion` | Plus an end-to-end benchmark script over fixture repos. |
| File watching (later) | `notify` | Only for the optional watch mode. |

Add dependencies deliberately. Each one affects binary size, build time, and startup.

## Storage: SQLite

- **The cache is disposable.** Everything in it can be rebuilt from source. A missing, corrupt, or version-mismatched database is deleted and rebuilt rather than migrated.
- **Location:** `.lintgraph/` at the project root, containing the database and a schema-version marker. Create a `.lintgraph/.gitignore` containing `*` so the directory ignores itself without touching the user's own `.gitignore`.
- **Pragmas:** WAL mode, `synchronous=NORMAL`, `foreign_keys=ON`, and a `busy_timeout` so concurrent runs (an editor hook and a git hook) wait instead of failing.
- **Schema version:** use `PRAGMA user_version`. A mismatch triggers a rebuild.
- **Writes:** One transaction per incremental update, using cached prepared statements. Replace the rows of changed files, then resolve.
- **Reads:** every rule query must be backed by an index. Check query plans (`EXPLAIN QUERY PLAN`) for new rules.
- **IDs:** integer primary keys for files and symbols. Store names as text, relationships as integer references.
- **Schema status: provisional.** Schema, indexes, and queries are expected to be optimized as the project learns what rules need. Keep them behind a storage module so changes stay local.

## Performance principles

Optimization is a design constraint from the start.

1. **Do not redo work for unchanged files.** Compare size and modified time first, then content hash. Re-resolve unchanged files only when they depend on a changed file.
2. **Startup is paid on every run.** Keep initialization lazy and minimal: no eager scanning, no large static tables built at startup.
3. **Parse in parallel, write serially.** Extraction runs on all cores. Database writes are batched in one transaction.
4. **Store resolved relationships, not raw text.** Rules query the graph and never reread source files.
5. **Measure before optimizing, and keep measuring.** A benchmark harness exists from the first phase.
6. **Budget memory.** Stream results where possible and avoid holding whole-project syntax trees in memory.

### Benchmark plan

- **Fixtures:** one small and one large open-source Python project, plus synthetic projects for edge cases.
- **Scenarios:** cold index, warm run with no changes, single-file edit, and many-file edit (such as a branch switch).
- **Track:** wall time, peak memory, and database size, recorded per commit in CI where practical.

### Targets (goals to validate, not promises)

- Process startup to first useful work: about 10 ms or less.
- Warm run with no changes, mid-size project (about 1,000 files): about 100 ms or less.
- Single-file edit, same project: about 200 ms or less.
- Cold full index, same project: a few seconds at most.

## Configuration philosophy

- **Convention over configuration.** Defaults, presets, and rule metadata live in code. The user's configs holds only deviations.
- **Zero-config start.** `lintgraph check` works without a config file; it detects Python, finds source roots (such as `src/`), and applies the recommended preset.
- **Format:** TOML. `lintgraph.toml` at the project root, with `[tool.lintgraph]` in `pyproject.toml` also accepted for Python projects.
- **Schema from code.** Config types are Rust structs. The JSON schema is generated from them and published as a build artifact for editor autocompletion and validation.
- **Settings and rules are separate.** `lintgraph.toml` holds project facts and overrides. Rule logic lives in code (core rules and language packs), so new rules do not add keys to user files.
- **Two classes of rules.** Universal rules (unresolved imports, cycles) rely on defaults. Project-specific rules (layering, custom conventions) are configuration by nature, and growth concentrates there.
- **Growth controls:**
    - Namespaced rule IDs allow bulk settings (`"python/*" = "warn"`).
    - Each rule declares its own typed options and defaults. Docs and validation derive from that metadata.
    - Presets with `extends`.
    - Path overrides (`[[overrides]]`) for areas such as `tests/**`.
    - Baselines (`lintgraph baseline`) record existing violations so legacy code can adopt Lintgraph without long exclude lists.
    - Inline suppressions: `# lintgraph: ignore[rule-id]`.
    - An `include` option to split large architecture rules into a separate file.
- **Validation:** unknown keys are errors, with line numbers and "did you mean" suggestions. Never ignore config silently.
- **Schema version:** a `schema-version` key allows warnings and migrations later.
- **Discipline:** every new option needs a default and a reason to exist. Treat the config surface like a public API, and prefer opinionated rules over configurable ones.

## CLI and UX

Good tooling from the earliest releases is a requirement.

- **Commands:** `lintgraph check` (default), `lintgraph init`, `lintgraph clean`, `lintgraph config check`, `lintgraph config show` (effective config), `lintgraph explain <rule-id>`, `lintgraph baseline`.
- **Flags:** `--changed-only` (reports findings in changed files and in unchanged files the change affected, such as an importer that no longer resolves), explicit file lists, `--format text|json|sarif`, `--fail-on <severity>`, `--no-color`, `--quiet`, `--verbose`. Respect `NO_COLOR`.
- **Exit codes:** `0` clean, `1` findings at or above the failure threshold, `2` usage or internal error.
- **Text output:** grouped by file, with line, column, rule ID, and a short snippet. End with a one-line summary.
- **JSON output:** stable and versioned (`format-version`). Fields include file, line, column, rule, severity, and message. Never break the schema without bumping the version.
- **Errors:** actionable messages that say what happened, why, and what to try. No raw stack traces for expected failures.
- **Speed of `--help`:** it should feel instant.
- **Project root discovery:** walk upward to find `lintgraph.toml` or `.git`.
- **Concurrency:** safe when several invocations run at once.

## Python adapter (first language pack)

- **Supported Python version:** parse any Python 3 syntax. Use a bundled stdlib module list for Python 3.9 and later.
- **Phase scope:** symbols, imports (absolute, relative, aliased, `from` imports, `__all__`), and name references first. Call edges come later.
- **Name references:** record every use of a name in load context (`foo`, `foo()`, decorators, base classes, default values, annotations) and attribute access on imported modules (`mod.foo`). Store them unresolved during extraction. The resolver links them to symbols. References are what `python/unreferenced-symbol` counts, so a function used only inside its own module is not reported as unused.
- **Layouts:** flat layout, `src/` layout, packages with `__init__.py`, and namespace packages.
- **Import classification:** internal, stdlib, third-party, or undeclared. Only unresolved **internal** imports are errors. Verification against a virtual environment is a later option, and other language packs may need it sooner.
- **Declared dependencies:** read them from `pyproject.toml` (`[project.dependencies]`, `[project.optional-dependencies]`, `[dependency-groups]`, and `[tool.poetry]` dependency tables). A top-level import that matches a declared dependency is third-party ("external, unverified"). One that matches nothing internal, stdlib, or declared is **undeclared**, and `python/unresolved-import` reports it as a warning: it is either a module that moved or a missing dependency.
- **Distribution names vs import names:** a package's install name often differs from its import name (`pillow` imports as `PIL`, `pyyaml` as `yaml`, `beautifulsoup4` as `bs4`). Normalize names first (lowercase, `-` and `.` to `_`), then consult a small bundled mapping for common mismatches. A `known-third-party` config option covers the rest.
- **No `pyproject.toml`:** classification falls back to internal, stdlib, or third-party, and nothing is reported as undeclared.
- **Out of scope:** dynamic imports, `import *` expansion beyond simple cases, and runtime metaprogramming. Report these as "unknown" instead of guessing.
- **Broken files:** index best-effort, flag the file, and never abort the whole run.

## Constraints
 
- **No network access and no telemetry.**
- **Never modify source files.**
- **Platforms:** Linux, macOS, and Windows, all in CI from the first phase. Pay particular attention to path handling, case sensitivity, and symlinks.
- **License:** MIT. Include a `LICENSE` file and set `license = "MIT"` in each crate manifest.
- **No custom type-checker.** Type-checkers (pyright, ty, mypy) will plug in later as external diagnostic sources whose output is normalized into Lintgraph's finding format.
- **No resident process in the core.** An optional watch mode and a thin editor wrapper may come later, but both call the same pipeline.

## Out of scope for suggestions
 
When generating code or ideas for this project, do not propose: LSP features (completion, hover, rename), an in-core daemon, network services, telemetry, a hand-written type-checker, or dependencies that conflict with the stack above without raising the trade-off first.
