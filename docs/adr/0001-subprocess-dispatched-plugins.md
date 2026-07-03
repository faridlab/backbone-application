# ADR-0001: Subprocess-dispatched plugins

- **Status:** Accepted
- **Deciders:** Metaphor maintainers
- **Applies to:** the `metaphor` CLI and its plugin binaries

## Context

The `metaphor` CLI surface is large — scaffolding, schema codegen, dev workflows, deployment, agent
installation — and each area has heavy, independent dependency trees (a codegen engine, a schema
parser, docker orchestration). Building all of that into one monolithic CLI binary would mean:

- Every change to any generator forces a rebuild and re-release of the whole CLI.
- Dependency conflicts between unrelated features (e.g. a proto toolchain vs. a docker client) all
  land in one `Cargo.toml`.
- A crash or a bug in one generator can take down the core CLI.

## Decision

Split the CLI's capabilities into **separate plugin binaries dispatched as subprocesses**:
`metaphor-codegen` (make, module, apps, proto, migration, seed), `metaphor-schema` (schema, webapp),
`metaphor-dev` (dev, lint, test, docs, config, jobs), and `metaphor-agent` (Claude Code skills /
subagents / CLAUDE.md). The core `metaphor` binary discovers and invokes them over the process
boundary. Discovery order: `$PATH` → `$METAPHOR_PLUGIN_BIN_DIR` → `~/.metaphor/bin/`.

## Consequences

**Positive**

- A plugin can be upgraded, swapped, or crash without rebuilding or destabilizing the core CLI.
- Each plugin owns its own dependency tree; conflicts stay contained.
- The plugin surface is extensible — new capabilities ship as new binaries.

**Negative / costs**

- Process-boundary overhead per invocation (serialization, spawn cost).
- Version-skew risk between the core CLI and a plugin must be managed (discovery + doctor checks).
- More binaries to distribute and keep on `PATH`.

## Notes

The rationale is documented for maintainers in `metaphor-cli/docs/architecture.md` ("why plugins are
subprocess-dispatched"). This ADR summarizes it at handbook altitude.
