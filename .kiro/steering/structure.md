# Project Structure

## Architecture overview

Lintel is a pipeline. A trigger starts a run, changes are detected, a language adapter updates a graph stored in SQLite, rules query the graph, and a reporter produces output. The core is language-agnostic. Everything language specific sits behind the adapter boundary, or its respective rule packs.

```
Trigger  (CLI hook, agent, CI)
    -> Change detector (file hashes, git diff)
    -> Language adapter (tree-sitter extraction, then project-wide resolution)
    -> Project graph (SQLite on disk)
    -> Rule engine (rules from rule packs, run as queries)
    -> Reporter (text, JSON, SARIF; exit code)
```

The process exits only after reporting. The graph on disk is only state, and it is a rebuildable cache.

## Fundamental concepts

- **Project root:** the directory containing `lintel.toml` or `.git`. All paths are relative to it.
- **Source file:** a file matched by an adapter, identified by path and content hash.
- **Change set:** the files added, modified, or deleted since the last run.
- **Adapter:** a per-language module that turns source files into graph data. It has two phases: extraction (per file, parallel) and resolution (project-wide, after extraction).
- **Symbol:** a named definition (module, function, class, constant) with a kind and location.
- **Edge:** a relationship between nodes, such as an import, a reference, or (later) a call.
- **Project graph:** files, symbols, and edges stored in relational tables. A graph is nodes plus relationships, and each edge type is a table of pairs.
- **Rule:** a check with an ID, default severity, description, typed options, and a query over the graph.
- **Rule pack:** a shareable group of rules, either language-agnostic or tied to one language.
- **Finding:** one reported problem: rule, severity, location, message, and optional related locations.
- **Diagnostic source (later):** an external tool, such as a type-checker, whose output is normalized into findings.
- **Reporter:** turns findings into text, JSON, or SARIF, and determines the exit code.
- **Baseline:** a recorded set of existing findings that are not reported again.

## Components
 
1. **Change detector:** compares size, modified time, and content hash against the stored `files` table. Produces the change set. Honors ignore rules and always skips `.lintel/`.
2. **Language adapter:** extracts symbols and imports from changed files, then resolves names to definitions across the project. Resolution is the hardest part and gets the most testing.
3. **Graph store:** owns the SQLite schema, migrations (rebuild on version change), incremental replacement of a changed file's rows, and typed query helpers.
4. **Rule engine:** loads enabled rules, runs their queries, applies severity, baselines, and suppressions, and produces findings.
5. **Rule packs:** collections of rules. `core` holds language-agnostic rules. `python` holds Python-specific rules.
6. **Config:** schema, loading, presets, validation, and generation of the JSON Schema.
7. **Reporter:** text, JSON, and SARIF output, plus exit codes.
8. **CLI:** argument parsing, command dispatch, and orchestration of the run.

## Repository Layout
 
A Cargo workspace. Dependencies point inward toward `lintel-core`.
 
```
lintel/
├── Cargo.toml                  # workspace manifest
├── crates/
│   ├── lintel-core/            # graph schema and store, change detection,
│   │                           # adapter trait, rule engine, finding types
│   ├── lintel-config/          # config schema, loading, presets, validation
│   ├── lintel-rules/           # language-agnostic rule pack ("core/")
│   ├── lintel-python-pack/     # Python adapter and Python rule pack ("python/")
│   ├── lintel-report/          # text, JSON, SARIF reporters
│   └── lintel-graph/           # CLI crate, published as `lintel-graph`,
│                               # installs the `lintel` binary
├── fixtures/                   # tiny Python projects with expected findings
├── benches/                    # criterion benchmarks and end-to-end scripts
├── docs/                       # user docs, rule reference, schema docs
└── .kiro/steering/             # product.md, tech.md, structure.md, roadmap.md
```
 
### Dependency direction
 
- `lintel-core` depends on no other Lintel crate and knows nothing about specific languages.
- `lintel-config`, `lintel-rules`, `lintel-python`, and `lintel-report` depend on `lintel-core`, never on each other where avoidable.
- `lintel-graph` depends on all of them and is the only crate that wires them together.
- Adding a language means adding a new crate (for example `lintel-typescript-pack`) that implements the adapter trait. It must not require changes in `lintel-core`.


## Adapter boundary
 
Illustrative sketch. The exact shape is provisional.
 
```rust
pub trait Adapter {
    /// Stable identifier, such as "python".
    fn id(&self) -> &'static str;
 
    /// Does this adapter apply to the project at `root`?
    fn detect(&self, root: &Path) -> bool;
 
    /// Source roots for the project (for example `src/` or the project root).
    fn source_roots(&self, root: &Path) -> Vec<PathBuf>;
 
    /// Does this adapter handle `path`?
    fn matches(&self, path: &Path) -> bool;
 
    /// Phase 1, per file and parallel: parse and extract symbols and imports.
    fn extract(&self, file: &SourceFile) -> Extraction;
 
    /// Phase 2, project-wide: resolve names to definitions and write edges.
    fn resolve(&self, store: &mut GraphStore, changed: &[FileId]) -> Result<()>;
}
```
 
Rules cannot reach past this boundary. They query the graph, never source files or syntax trees.

## Graph schema

A first sketch to guide early work. Expect it to change as rules and optimization needs become clear. Keep all access behind the storage module.
 
| Table | Purpose | Key columns |
|---|---|---|
| `files` | Tracked source files | `id`, `path`, `hash`, `size`, `mtime`, `language`, `parse_status` |
| `symbols` | Definitions | `id`, `file_id`, `name`, `qualified_name`, `kind`, `line`, `exported` |
| `imports` | Import statements | `id`, `file_id`, `target_module`, `target_file_id`, `imported_name`, `resolved_symbol_id`, `kind` (internal, stdlib, third-party, unknown), `line` |
| `references` | Uses of symbols (later: call edges with arity) | `id`, `file_id`, `symbol_id`, `line` |
| `diagnostics` (later) | Normalized findings from external tools | `id`, `file_id`, `source`, `rule`, `severity`, `line`, `message` |
 
Conventions: integer IDs, foreign keys on, an index for every column a rule filters or joins on, and deletion of a file's rows (cascading) before re-inserting its fresh extraction.

## Rules
 
- **IDs:** `<scope>/<name>` in kebab-case. Scopes: `core` for language-agnostic rules, `python` for Python rules. Examples: `python/unresolved-import`, `core/circular-import`, `python/unreferenced-symbol`.
- **Metadata:** each rule declares its ID, category, default severity, short description, longer explanation (shown by `lintel explain`), and typed options with defaults.
- **Implementation:** a rule is a query plus light post-processing. It reads only from the graph.
- **Severities:** `error`, `warn`, `info`.
- **Stability:** a rule ID is part of the public interface. Renaming one is a breaking change.

## Finding format (JSON, versioned)
 
```json
{
  "format-version": 1,
  "findings": [
    {
      "rule": "python/unresolved-import",
      "severity": "error",
      "file": "src/app/main.py",
      "line": 3,
      "column": 1,
      "message": "Cannot resolve internal module `app.utils.helpers`.",
      "related": []
    }
  ],
  "summary": { "errors": 1, "warnings": 0, "files-checked": 42 }
}
```
 
## Runtime layout
 
```
.lintel/
├── .gitignore        # contains "*", so the directory ignores itself
├── graph.db          # SQLite cache (disposable)
└── schema-version    # cache schema marker
```
 
## Naming conventions
 
- **Crates:** kebab-case with a `lintel-` prefix. Directory names match crate names.
- **Rust code:** standard Rust style (`snake_case` functions and modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants).
- **CLI flags, config keys, rule IDs:** kebab-case.
- **JSON keys:** kebab-case, to match the config.
- **Files in `fixtures/`:** one directory per scenario, named for what it demonstrates (for example `circular-import-basic`), with an `expected.json` snapshot.

## Code organization patterns
 
- Keep modules small and single-purpose. A new rule is a new file registered in its pack.
- Public APIs of each crate are narrow. Expose traits and types, and keep implementation details private.
- Keep I/O at the edges: the CLI handles the process and terminal, `lintel-core` takes explicit inputs and returns values.
- Storage access goes through typed helpers. No ad-hoc SQL scattered across crates.
- Parallelism lives in the extraction phase. Database writes stay on one thread.

## Testing structure
 
- **Unit tests** beside the code for resolvers, parsers, and rule logic.
- **Fixture tests:** each fixture project is indexed and checked, and findings are compared against `expected.json` with snapshot tests.
- **CLI tests** with `assert_cmd`, covering exit codes, output formats, and error messages.
- **Incremental tests:** edit a file, rerun, and assert only that file was re-extracted and the graph matches a fresh full index.
- **Cross-platform CI:** Linux, macOS, and Windows from the start.
- **Benchmarks:** criterion for hot paths, plus end-to-end runs on reference repositories.


## Extension points
 
- **New language:** implement the adapter trait in a new crate and add a rule pack if it has language-specific rules.
- **New rule:** add a file to a pack with metadata and a query.
- **New output format:** add a reporter in `lintel-report`.
- **New diagnostic source (later):** run an external tool, normalize its output into findings, and optionally store it in `diagnostics`.
- **Shared rule packs (later):** distribute packs as files or crates that register rules at startup.