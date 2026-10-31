# Lintgraph

<p align="left">
    <img alt = "Rust" src="https://img.shields.io/badge/rust-%23F54A2A.svg?style=for-the-badge&logo=rust&logoColor=white"/>
    <img alt = "SQLite" src="https://img.shields.io/badge/sqlite-%235091CD.svg?style=for-the-badge&logo=sqlite&logoColor=white"/>
</p>

**Lintgraph** is a fast, project-aware static checker. It builds a lightweight graph of your whole codebase, runs checks against it after you write code, reports what it finds, and exits. It sits between a linter and a language server: it sees across files like an LSP does, but it keeps no resident process and no live model in memory.

## Why it exists

- **Linters** are simple and fast, but they mostly see one file at a time. They miss broken imports, dead exports, import cycles, and layering violations.
- **Language servers** see the whole project, but they run as heavy, long-lived processes built for interactive editing.
- **AI agents and CI** need something different: a quick answer to "did this edit break anything across the project?", in a structured format.

Lintgraph aims at the gap between them.

## How it will work

1. Parse changed files with **tree-sitter**.
2. Extract symbols and imports, and resolve them across the project.
3. Store the result as a graph in a local SQLite cache.
4. Run rules as queries over the graph.
5. Report as text or JSON, then exit.

The cache is disposable and rebuilt from source whenever needed. Lintgraph only reads your code and never runs it.

## Planned usage

```sh
lintgraph init                    # create lintgraph.toml in the project root
lintgraph check                   # check the project
lintgraph check --changed-only    # report on what a change touched or broke
lintgraph check --format json     # structured output for agents and CI
```

## Status

Rust is the implementation language, and Python is the first supported language. Language support will ship as `lintgraph-<language>-pack` crates, each bundling an adapter and its rules.