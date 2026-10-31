---
inclusion: auto
name: roadmap
description: Lintel's phased prototype plan (Phases 0 to 4), deliverables, done-when checks, post-prototype ideas, and risks. Use when planning work, choosing what to build next, or implementing a roadmap deliverable.
---

# Roadmap

A phased plan for the prototype. The first five phases (0 to 4) define the prototype. Each phase has a "done when" check. Everything after Phase 4 is expansion on a modular base.

Across all phases:

- Keep the core language-agnostic and the language pack boundary clean.
- Add or extend benchmarks and fixtures in every phase, not at the end.
- Invest in UX (messages, output, help) as features land.
- Treat the schema, queries, and tables as provisional, and keep them behind the storage module.

## Phase 0: Decisions

Settle the choices that everything else depends on, and write them down.

**Decided**

- Implementation language: Rust. First language pack: Python (`lintel-python-pack`).
- Language support ships as `lintel-<language>-pack` crates, each bundling the adapter and that language's rules, compiled in through Cargo features.
- Storage: SQLite (bundled), used as a disposable cache.
- Config: TOML, with defaults in code and a published JSON Schema.
- Project name: Lintel. Registry name: `lintel-graph`. Command: `lintel`. License: MIT.
- First rules: `python/unresolved-import`, `core/circular-import`, `python/unreferenced-symbol`.
- Python imports are classified as internal, stdlib, or third-party. Only unresolved internal imports are errors.
- Platforms: Linux, macOS, and Windows from the start.
- Defaults: exit codes `0`, `1`, `2`; best-effort indexing of files with syntax errors; busy timeout for concurrent runs; no network or telemetry; `--changed-only` and versioned JSON output.

**To finalize in this phase**

- Confirm that the name `lintel-graph` is free on crates.io (`lintel-check` was found to be taken), and note GitHub and domain availability.
- Graph schema v0 on paper (see `structure.md`).
- Language pack interface (adapter plus rule registration) and the resolver's inputs and outputs.
- Finding JSON format v1 and the rule ID scheme.
- Choose the reference repositories (one small, one large) and the benchmark scenarios.

**Done when:** the decisions above are recorded, and the repository can be created without revisiting them.

## Phase 1: Skeleton

Build the project shell so everything else has a place to live, with good UX from the start.

**Deliverables**

- Cargo workspace with the crates from `structure.md` (stubs where needed).
- CLI with `lintel check`, `lintel init`, `lintel clean`, and `lintel config check` and `show`.
- Project root discovery, and `.lintel/` creation with its self-ignoring `.gitignore`.
- Config loading: zero-config defaults, `lintel.toml`, `[tool.lintel]` in `pyproject.toml`, strict validation with line numbers and "did you mean" suggestions.
- File walker honoring `.gitignore`, always skipping `.lintel/`.
- Change detection: size and modified time first, then content hash.
- SQLite setup: pragmas, `user_version` schema marker, rebuild on mismatch, a `files` table.
- Error type conventions and readable error messages.
- CI on Linux, macOS, and Windows, with `rustfmt`, `clippy`, and tests.
- Benchmark harness and the chosen fixture repositories.

**Done when:** `lintel check` on a Python project scans files, detects changes, and persists the file table. A second run reports nothing changed and meets the warm-run target. CI is green on all three platforms.

## Phase 2: Adapter

Teach Lintel to read Python, and fill the graph.

**Deliverables**

- Language pack interface and adapter trait in `lintel-core`, and the Python adapter inside `lintel-python-pack`.
- tree-sitter parsing for Python, tolerant of syntax errors (flag the file, continue).
- Extraction of module-level symbols (functions, classes, constants) and imports (absolute, relative, aliased, `from` imports, `__all__`).
- Extraction of name references: every use of a name in load context, plus attribute access on imported modules (`mod.foo`). Stored unresolved, with name, line, and column. Phase 3 resolves them to symbols.
- Parallel per-file extraction with `rayon`, and serial batched writes in one transaction.
- Incremental updates: replace only the changed files' rows, and handle deleted and renamed files.
- `symbols`, `imports`, and `references` tables, with indexes for the queries Phase 3 needs.
- Fixture projects and snapshot tests for the extraction output, including references used only within their own module.

**Done when:** the graph for each fixture project is correct, including references, editing one file re-extracts only that file, and an incremental graph matches a fresh full index.

## Phase 3: Resolver and First Rules

Connect the pieces: resolve imports across the project and run the first rules.

**Deliverables**

- Module-to-file mapping for flat and `src/` layouts, packages, and namespace packages.
- Relative import resolution, and resolution of `from x import name` to a symbol where possible.
- Import classification (internal, stdlib, third-party, undeclared) using a bundled stdlib module list for Python 3.9 and later and the dependencies declared in `pyproject.toml`.
- Resolution of extracted name references to symbols, so `python/unreferenced-symbol` can count uses within a module as well as across modules.
- Honest "unknown" results for dynamic imports and unsupported patterns.
- Rule engine in `lintel-core`: rule metadata, typed options, severity, execution of query-based rules, and registration of rules from language packs and `lintel-rules`.
- First rules (`python/` rules live in `lintel-python-pack`, `core/` rules in `lintel-rules`):
  - `python/unresolved-import`: internal imports that resolve to nothing (error), and undeclared imports that match no internal module, stdlib module, or declared dependency (warning).
  - `core/circular-import`: import cycles between modules.
  - `python/unreferenced-symbol`: module-level definitions that nothing references. This ships conservative and documented, with exemptions for `__all__`, decorators, entry points, and test conventions. It is marked experimental in `lintel explain`. It can move to a later phase without blocking the others if the false-positive rate is too high.
- Fixture projects for each rule, including tricky cases (re-exports, `__init__.py` imports, conditional imports).

**Done when:** all three rules pass their fixture snapshots, and findings on the two reference repositories are manually triaged with acceptable noise, especially for `python/unresolved-import`.

## Phase 4: Reporting

Make the output genuinely usable by humans, hooks, CI, and agents.

**Deliverables**

- Text reporter: grouped by file, line and column, rule ID, short snippet, and a one-line summary. Color support and `NO_COLOR`.
- JSON reporter: stable and versioned (`format-version: 1`).
- SARIF output for tools that consume it.
- Exit codes, `--fail-on <severity>`, `--quiet`, and `--verbose`.
- `--changed-only` and explicit file-list arguments.
- `lintel explain <rule-id>`.
- Inline suppressions (`# lintel: ignore[rule-id]`) and `lintel baseline`.
- Generated JSON Schema for the config, and a docs page for rules and configuration.
- Example integrations: a git pre-commit hook and a post-edit command for an agent.

**Done when:** Lintel is usable in a git hook and by an agent with no manual setup beyond installing it, and its output is stable enough to document.

## After the prototype

Ordered roughly by value. Nothing here is committed until the earlier phases prove the core.

- Call edges and arity checks.
- Architecture rules: layer definitions, forbidden dependencies, custom conventions (with the `include` option for large rule sets).
- Deep tier: external type-checkers (pyright, ty, mypy) as diagnostic sources, normalized into findings and stored in the graph.
- Read-only queries: `lintel refs <symbol>` and "what is affected if this changes?".
- Optimization pass on schema, indexes, and queries, driven by benchmark data.
- Optional `lintel watch` with debounced filesystem events, reusing the same pipeline.
- Thin editor integrations: SARIF and problem matchers first, then an extension or a small language-server wrapper that shells out to the CLI.
- A second language pack (for example `lintel-typescript-pack`) to test the modularity of the core.
- Move language packs into their own repositories once the pack interface and graph schema are stable and a second pack exists.
- Shared rule sets and presets (declarative project rules distributed as files).
- Third-party import verification against a virtual environment.

## Risks

- **Resolver complexity.** Python's import system has many corners. Mitigation: build it early, keep the first rules simple, and report "unknown" instead of guessing.
- **False positives.** Especially for `python/unreferenced-symbol`. Mitigation: conservative defaults, experimental labeling, and baselines.
- **Schema churn.** Mitigation: treat the schema as provisional, hide it behind the storage module, and rebuild the cache on version changes.
- **Config growth.** Mitigation: the growth controls in `tech.md`, and tracking the size of typical configs.
- **Scope creep toward an LSP.** Mitigation: the non-goals in `product.md`.
- **Name availability.** Mitigation: confirm in Phase 0, and keep the binary name independent of the registry name.