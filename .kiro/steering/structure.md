---
inclusion: always
---

# Project Structure

## Architecture overview

Lintgraph is a pipeline. A trigger starts a run, changes are detected, a language adapter updates a graph stored in SQLite, rules query the graph, and a reporter produces output. The core is language-agnostic. Everything language-specific sits in a language pack: its adapter and its language rules.

```
Trigger  (CLI hook, agent, CI)
    -> Change detector (file hashes, git diff)
    -> Language adapter (tree-sitter extraction, then project-wide resolution)
    -> Project graph (SQLite on disk)
    -> Rule engine (core rules plus language pack rules)
    -> Reporter (text, JSON, SARIF; exit code)
```

The process exits only after reporting. The graph on disk is only state, and it is a rebuildable cache.

## Fundamental concepts

- **Project root:** the directory containing `lintgraph.toml`. All paths are relative to it.
- **Source file:** a file matched by an adapter, identified by path and content hash.
- **Change set:** the files added, modified, or deleted since the last run.
- **Affected set:** the change set plus every file whose resolution can depend on it: files that import a changed file, and files with unresolved imports that a new or renamed file might now satisfy. This is what gets re-resolved and what `--changed-only` reports on.
- **Adapter:** a per-language module that turns source files into graph data. It has two phases: extraction (per file, parallel) and resolution (project-wide, after extraction).
- **Symbol:** a named definition (module, function, class, constant) with a kind and location.
- **Edge:** a relationship between nodes, such as an import, a reference, or (later) a call.
- **Project graph:** files, symbols, and edges stored in relational tables. A graph is nodes plus relationships, and each edge type is a table of pairs.
- **Rule:** a check with an ID, default severity, description, typed options, and logic that reads only from the graph.
- **Core rules:** built-in, language-agnostic rules (`core/`). They work on the files, symbols, and edges that every adapter produces, so they apply to any supported language. They are the sane defaults users get without configuring anything.
- **Language pack:** a crate named `lintgraph-<language>-pack` that bundles one language's adapter and its language-specific rules (for example `python/`). One pack per language.
- **External rule pack (later):** a group of extra rules distributed outside Lintgraph and loaded on top of the core rules and language packs. It can hold language-specific rules beyond those a language pack ships, and rules for a convention (such as a framework's or a team's).
- **Finding:** one reported problem: rule, severity, location, message, and optional related locations.
- **Diagnostic source (later):** an external tool, such as a type-checker, whose output is normalized into findings.
- **Reporter:** turns findings into text, JSON, or SARIF, and determines the exit code.
- **Baseline:** a recorded set of existing findings that are not reported again.

## Components
 
1. **Change detector:** compares size, modified time, and content hash against the stored `files` table. Produces the change set, then expands it to the affected set using the reverse lookup on `imports.target_file_id` and the stored unresolved imports. Honors ignore rules and always skips `.lintgraph/`.
2. **Language adapter:** extracts symbols, imports, and references from changed files, then resolves names to definitions for the whole affected set. Resolution is the hardest part and gets the most testing.
3. **Graph store:** owns the SQLite schema, migrations (rebuild on version change), incremental replacement of a changed file's rows, and typed query helpers.
4. **Rule engine:** loads enabled rules, runs them, applies severity, baselines, and suppressions, and produces findings.
5. **Rules:** core rules in `lintgraph-rules` (`core/`), and language-specific rules inside each language pack (`python/` in `lintgraph-python-pack`).
6. **Config:** schema, loading, presets, validation, and generation of the JSON Schema.
7. **Reporter:** text, JSON, and SARIF output, plus exit codes.
8. **CLI:** argument parsing, command dispatch, and orchestration of the run.

## Repository Layout
 
A Cargo workspace. Dependencies point inward toward `lintgraph-core`.
 
```
lintgraph/
├── Cargo.toml                  # workspace manifest
├── crates/
│   ├── lintgraph-core/            # graph schema and store, change detection,
│   │                           # adapter trait, rule engine, finding types
│   ├── lintgraph-config/          # config schema, loading, presets, validation
│   ├── lintgraph-rules/           # core rules, language-agnostic ("core/")
│   ├── lintgraph-python-pack/     # Python language pack: adapter and rules ("python/")
│   ├── lintgraph-report/          # text, JSON, SARIF reporters
│   └── lintgraph/              # CLI crate, published as `lintgraph`,
│                               # installs the `lintgraph` binary
├── fixtures/                   # tiny Python projects with expected findings
├── benches/                    # criterion benchmarks and end-to-end scripts
├── docs/                       # user docs, rule reference, schema docs
└── .kiro/steering/             # product.md, tech.md, structure.md, roadmap.md
```
 
### Dependency direction
 
- `lintgraph-core` depends on no other Lintgraph crate and knows nothing about specific languages.
- `lintgraph-config`, `lintgraph-rules`, `lintgraph-python-pack`, and `lintgraph-report` depend on `lintgraph-core`, never on each other where avoidable.
- `lintgraph` (the CLI crate) depends on all of them and is the only crate that wires them together.
- Adding a language means adding a new crate (for example `lintgraph-typescript-pack`) that implements the adapter trait. It must not require changes in `lintgraph-core`.


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
 
    /// Phase 1, per file and parallel: parse and extract symbols, imports, and references.
    fn extract(&self, file: &SourceFile) -> Extraction;
 
    /// Phase 2, project-wide: resolve names to definitions and write edges
    /// for every file in the affected set, not only the changed files.
    fn resolve(&self, store: &mut GraphStore, affected: &[FileId]) -> Result<()>;
}
```

The core computes the affected set before calling `resolve`, so adapters do not each reimplement dependent tracking.
 
Rules cannot reach past this boundary. They query the graph, never source files or syntax trees.

## Graph schema

A first sketch to guide early work. Expect it to change as rules and optimization needs become clear. Keep all access behind the storage module.
 
| Table | Purpose | Key columns |
|---|---|---|
| `files` | Tracked source files | `id`, `path`, `hash`, `size`, `mtime`, `language`, `parse_status` |
| `symbols` | Definitions | `id`, `file_id`, `name`, `qualified_name`, `kind`, `line`, `exported` |
| `imports` | Import statements | `id`, `file_id`, `target_module`, `target_file_id`, `imported_name`, `resolved_symbol_id`, `kind` (internal, stdlib, third-party, undeclared, unknown), `line` |
| `references` | Uses of names, extracted in Phase 2 and resolved to symbols in Phase 3 (later: call edges with arity) | `id`, `file_id`, `name`, `symbol_id` (null until resolved), `line`, `column` |
| `diagnostics` (later) | Normalized findings from external tools | `id`, `file_id`, `source`, `rule`, `severity`, `line`, `message` |
 
Conventions: integer IDs, foreign keys on, an index for every column a rule filters or joins on, and deletion of a file's rows (cascading) before re-inserting its fresh extraction.

## Rules
 
- **IDs:** `<scope>/<name>` in kebab-case. Scopes: `core` for core rules, and the language ID (such as `python`) for a language pack's rules. Examples: `python/unresolved-import`, `core/circular-import`, `python/unreferenced-symbol`.
- **What belongs in core:** a rule is a core rule only if it needs nothing beyond files, symbols, and edges. Cycles, layering, and forbidden dependencies qualify. Anything that depends on language semantics (such as `__all__` or decorators) belongs in the language pack.
- **Metadata:** each rule declares its ID, category, default severity, short description, longer explanation (shown by `lintgraph explain`), and typed options with defaults.
- **Implementation:** a rule loads data from the graph with indexed queries, then does its work in Rust. Simple rules are a query plus light post-processing. Rules that need a graph algorithm load the relevant edges and run it in memory: `core/circular-import` loads import edges and finds strongly connected components (Tarjan's algorithm), reporting each cycle once with its modules as related locations. Rules never read source files or syntax trees.
- **Severities:** `error`, `warn`, `info`. In config, a rule can also be set to `off`. Severity alone does not fail a run; `--fail-on` decides that.
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
.lintgraph/
├── .gitignore        # contains "*", so the directory ignores itself
├── graph.db          # SQLite cache (disposable)
└── schema-version    # cache schema marker
```
 
## Naming conventions
 
- **Crates:** kebab-case with a `lintgraph-` prefix, except the CLI crate, which is named `lintgraph`. Directory names match crate names.
- **Rust code:** standard Rust style (`snake_case` functions and modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants).
- **CLI flags, config keys, rule IDs:** kebab-case.
- **JSON keys:** kebab-case, to match the config.
- **Files in `fixtures/`:** one directory per scenario, named for what it demonstrates (for example `circular-import-basic`), with an `expected.json` snapshot.

## Code organization patterns
 
- Keep modules small and single-purpose. A new rule is a new file registered in `lintgraph-rules` or its language pack.
- Public APIs of each crate are narrow. Expose traits and types, and keep implementation details private.
- Keep I/O at the edges: the CLI handles the process and terminal, `lintgraph-core` takes explicit inputs and returns values.
- Storage access goes through typed helpers. No ad-hoc SQL scattered across crates.
- Parallelism lives in the extraction phase. Database writes stay on one thread.

## Testing structure
 
- **Unit tests** beside the code for resolvers, parsers, and rule logic.
- **Fixture tests:** each fixture project is indexed and checked, and findings are compared against `expected.json` with snapshot tests.
- **CLI tests** with `assert_cmd`, covering exit codes, output formats, and error messages.
- **Incremental tests:** edit a file, rerun, and assert only that file was re-extracted, its dependents were re-resolved, and the graph matches a fresh full index. Include breakage in an unchanged importer (remove a symbol another file imports) and repair (add a file that satisfies a previously unresolved import).
- **Cross-platform CI:** Linux, macOS, and Windows from the start.
- **Benchmarks:** criterion for hot paths, plus end-to-end runs on reference repositories.


## Extension points
 
- **New language:** add a `lintgraph-<language>-pack` crate that implements the adapter trait and registers that language's rules.
- **New rule:** add a file with metadata and logic to `lintgraph-rules` (core) or a language pack.
- **New output format:** add a reporter in `lintgraph-report`.
- **New diagnostic source (later):** run an external tool, normalize its output into findings, and optionally store it in `diagnostics`.
- **External rule packs (later):** distribute extra language-specific rules and convention rules as files or crates that register rules at startup.