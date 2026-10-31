---
inclusion: auto
name: product
description: Lintgraph's problem statement, target users, design principles, goals, and non-goals. Use when proposing features, judging scope, or explaining why Lintgraph exists.
---

# Product Overview

## Summary

Lintgraph is a fast, project-aware static checker. It runs on demand, checks a whole codebase through a persistent, queryable graph, reports its findings, and exits. It sits between a linter and a language server: it understands relationships across files, as an LSP does, but it keeps no resident process and no live in-memory model.

- **Project name:** Lintgraph
- **Registry (crates.io) name:** `lintgraph`
- **Command:** `lintgraph`
- **Implementation language:** Rust
- **First supported language:** Python
- **Status:** Prototype. Scope and schema are provisional unless stated otherwise.

## Problem

### Linters are simple, and blind beyond one file

Linters are cheap to run and easy to adopt: run, report, exit. Most of them reason about a single file at a time. That makes them fast and easy to parallelize, but it also makes them blind to the problems that matter most as a codebase grows:

- Imports that no longer resolve after a file was moved or renamed.
- Exports and functions that nothing uses anymore.
- Circular dependencies between modules.
- Layering violations, such as UI code importing the database layer.
- Calls that no longer match a function's current signature.

The linters that do look across files (for example, Pylint, or typescript-eslint with type information) get much slower and heavier, which erodes the simplicity that made linters attractive.

### Language servers are powerful, and heavy

Language servers provide project-wide intelligence, but they pay for it with a long-running process that holds a full semantic model in memory. Java servers must resolve build configuration, pull in dependencies, and index everything before they can answer anything. The same pattern appears with rust-analyzer and others. On top of that, editors that host them (often Electron-based) have their own memory cost.

Language servers are also designed for interactive editing: diagnostics on every keystroke, completion, hover, rename. Many of those features go unused much of the time, and their overhead is paid regardless.

### AI-assisted development sharpens both problems

Coding agents and automated workflows need a different kind of feedback: "did my edit break anything across the project?", asked once per edit, answered in structured form. They do not need hover documentation or keystroke-level updates. Linters cannot answer the question, and language servers answer it in an expensive, editor-shaped way. Memory pressure from large language servers and heavy editors also competes with the resources that local tooling and models need.

There is a gap between "one file, shallow" and "whole project, heavy". Lintgraph is built for that gap.

### Existing technologies and why they fall short

| Category | Examples | Strength | Why it does not cover the gap |
|---|---|---|---|
| Linters | Ruff, Flake8, ESLint | Fast, simple, run-and-exit | Largely file-oriented. Cannot see broken imports across modules, dead exports, cycles, or layering violations. Project-aware modes exist but are slow and heavy. |
| Type-checkers and compilers in check mode | pyright, mypy, `tsc --noEmit`, `cargo check` | Authoritative on types | Re-analyze parts of the project on each run, need a working build or environment, stop at "does it type-check", offer no architecture or dead-code rules, and each has its own output format. |
| Language servers | pyright and Pylance, jdtls, rust-analyzer, clangd | Rich, project-aware features | Resident processes holding a full semantic model, with heavy startup and memory costs. Built for interactive editing, which is the wrong shape for agents and CI. Tied to an editor. |
| Code indexes | SCIP, LSIF | Cheap queries over precomputed symbols | Built for code navigation, not checking. No rule engine. |
| Structural pattern tools | ast-grep, Semgrep | Fast, flexible structural matching | Mostly per-file. Cross-file analysis is limited or in commercial tiers. |
| Whole-program analyzers | CodeQL, Infer, Joern | Deep, project-wide analysis | Heavy database builds and slow runs, built for audits rather than fast feedback loops. |
| Single-purpose Python tools | import-linter, vulture, deptry | Each targets one problem well | Each builds its own view of the project, with its own config and output. No shared graph, no common rule model. |

None of these combine whole-project awareness, low resting cost, run-and-exit behavior, and a uniform, structured output.

## Proposed solution

Lintgraph builds a lightweight, persistent graph of a codebase (files, symbols, imports, and later call edges), runs rules as queries over that graph, and exits.

1. **Parse** changed files with tree-sitter, which is fast, incremental, and tolerant of broken code.
2. **Extract and resolve** symbols and relationships through a per-language adapter.
3. **Store** the result in a SQLite graph on disk, updated incrementally from file hashes.
4. **Check** the graph with rules: built-in core rules that work for any language, plus the rules in each language pack.
5. **Report** in human-readable text and a stable, versioned JSON format, with meaningful exit codes.

The process holds no state between runs. The graph on disk is the only memory, and it is a disposable cache that can always be rebuilt.

### Design principles

- **Convention over configuration.** Sensible defaults live in code. Configuration holds only deviations from them.
- **Fast by default.** Optimization is a design constraint from the start, not a later pass.
- **Static analysis only.** Lintgraph never executes the code it checks.
- **Stateless process.** No daemon in the core. Something else (an editor, a hook, an agent, CI) invokes it.
- **Honest about uncertainty.** When resolution is ambiguous, Lintgraph reports "unknown" instead of guessing.
- **Modular.** Each language ships as a separate language pack (adapter plus rules). The core knows nothing about any specific language.
- **Read-only.** Lintgraph never modifies source files.
- **Private.** No network access and no telemetry.

## Target users

- Developers and teams who want cross-file checks without running a language server.
- AI coding agents and automated workflows that need a fast, structured "did this edit break the project?" check.
- CI pipelines and git hooks that need a cheap gate before the compiler or test run.

## Benefits

### For human developers

- Cross-file breakage is caught right after writing code, before a build or a review.
- Low resting memory and battery cost compared with a resident language server.
- Works on code that does not build or is half-written.
- Identical results in any editor, terminal, or CI job, since the CLI is the source of truth.
- Project architecture rules (layering, forbidden dependencies) can be enforced automatically.
- A small, readable configuration with sensible defaults.

### For AI agents and automation

- Feedback shaped for the task: one project-wide check per edit instead of editor-oriented features.
- Structured, stable, versioned JSON output that is easy to act on.
- A fast loop that catches errors before long builds or compounding mistakes.
- No session state: a fresh sandbox can rebuild the graph from source.
- Small, targeted reports instead of large compiler dumps, which saves tokens.
- `--changed-only` and explicit file lists to focus on what an edit touched, including unchanged files the edit broke.

## Goals

- Provide cross-file checks that linters cannot, at a fraction of an LSP's resting cost.
- Make the first run useful with zero configuration.
- Keep warm runs fast enough to run on every save, hook, or agent edit (targets are in `tech.md`).
- Make adding a language (an adapter) or a rule (a query) a contained piece of work.
- Offer excellent tooling and error messages from the earliest releases.

## Non-goals

- **No editor features:** no completion, hover, rename, or refactoring. These need exact symbol resolution and live buffers, which are the costs Lintgraph avoids.
- **No custom type-checker:** existing type-checkers will later plug in as external diagnostic sources.
- **No dynamic analysis:** Lintgraph does not run, trace, or profile programs.
- **No resident process in the core:** an optional watch mode may come later, as a separate opt-in feature.
- **No source rewriting:** Lintgraph should not alter source code or auto-fix any flagged issues.

A later, read-only extension is likely, since the graph already holds the data: commands such as `lintgraph refs <symbol>` and "what is affected if this changes?".

## Success measures

- A typical project's `lintgraph.toml` is empty or only a few lines. A long typical config means the defaults are wrong. If many projects need the same long config, it should become a preset.
- Warm no-change and single-file-edit runs meet the performance targets in `tech.md`.
- Findings on the reference Python projects are low-noise after manual triage.
- A new language adapter does not require big changes to the core.

## Related documents

- `tech.md`: technology stack, performance principles, configuration philosophy, CLI behavior.
- `structure.md`: architecture, components, repository layout, conventions.
- `roadmap.md`: phased plan from decisions to the first rules and reporting.
