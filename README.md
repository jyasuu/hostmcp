# hostmcp

A Rust, dependency-light HTTP MCP server that gives an agent a standard set
of tools to operate a remote host: `read`, `edit`, `glob`, `grep`, `list`,
`bash`, exposed over the MCP Streamable-HTTP transport. Tool semantics are
ported from opencode's agent tool implementations (`read`/`edit`/`glob`/
`grep`/`bash`). No Node/Bun runtime required; single binary, `axum` for
HTTP, `ignore`/`regex`/`glob` for search.

## Build & run

```sh
cargo build --release
./target/release/hostmcp --root /path/to/project --port 8787
```

Flags (all optional):

| Flag      | Default   | Meaning                                                      |
|-----------|-----------|---------------------------------------------------------------|
| `--host`  | `0.0.0.0` | Bind address                                                  |
| `--port`  | `8787`    | Bind port                                                     |
| `--root`  | `.`       | Root that relative tool paths resolve against. Absolute paths passed by the caller are used as-is (matches opencode: tools can touch anything the process has permission for). |

Health check: `GET /health` -> `ok`.

## Protocol

Implements MCP's Streamable HTTP transport (spec `2025-06-18`) on a single
`POST /mcp` endpoint, JSON-RPC 2.0, methods: `initialize`, `ping`,
`tools/list`, `tools/call`, plus the `notifications/*` no-ops. `GET /mcp`
(server->client SSE stream) returns `405`, since this server never pushes
unsolicited messages — every response comes back synchronously on the POST.

Session handling: on `initialize`, if the caller didn't send
`Mcp-Session-Id`, the server mints one and returns it in the response
header. It's used only to scope the `edit` tool's "must `read` before
`edit`" bookkeeping (see below) — pass it back on later calls to keep that
state; if you never send one, the server falls back to a shared `default`
session, which is fine for single-user/local use.

## Tools

### `read`
`{ filePath, offset?, limit? }` — reads a file (numbered lines,
`offset`/`limit`, 2000-char line truncation, default 2000-line window) or
lists a directory (one entry per line, dirs suffixed `/`). Marks the file as
"read" in the session for `edit`'s fidelity check below. Missing files get a
"did you mean" suggestion scanned from the sibling directory, like upstream.

### `edit`
`{ filePath, oldString, newString, replaceAll? }` — exact string
replacement. Mirrors opencode's guardrails: the file must have been `read`
in the same session first; fails if `oldString` isn't found, and fails on
ambiguous (multiple-match) replacements unless `replaceAll` is set.

### `glob`
`{ pattern, path? }` — fast filename pattern search (`**/*.rs` etc.),
`.gitignore`-aware via the `ignore` crate, results sorted newest-first,
capped at 100 with a truncation note.

### `grep`
`{ pattern, path?, include? }` — regex content search (Rust `regex`
syntax) over a tree (or a single file), `.gitignore`-aware, optional
filename filter, capped at 100 matches, grouped output by file.

### `list`
`{ path?, depth? }` — recursive directory tree (default depth 3,
`.gitignore`-aware, capped at 500 entries). opencode doesn't ship this as a
separate tool (`read` handles single-level directory listing); this is a
tree-view addition to satisfy the requested tool surface.

### `bash`
`{ command, description?, timeout?, cwd? }` — runs `sh -c <command>`,
default 120s / max 600s timeout (kills the process group on timeout),
stdout/stderr captured separately and each truncated to 30KB.

## Wiring it up

Since this is HTTP rather than stdio, point any MCP-over-HTTP-capable client
at `http://<host>:<port>/mcp`. For stdio-only clients (e.g. Claude Desktop's
classic config), bridge with `mcp-remote`:

```json
{
  "mcpServers": {
    "hostmcp": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://localhost:8787/mcp"]
    }
  }
}
```

## Notes / deliberate simplifications vs. upstream opencode

- No LSP warm-up, no permission-prompt/`ctx.ask` layer, no image/PDF
  attachment decoding for `read` — binary files are reported, not decoded.
- `edit` doesn't produce opencode's rich diff metadata, just a compact
  `-`/`+` preview; the important behavioral guarantees (must-read-first,
  ambiguous-match rejection) are preserved.
- No sandboxing of `bash`/paths — this server has the same filesystem reach
  as the process running it. Run it scoped to a directory you trust, and put
  it behind whatever network boundary you'd put any local dev tool behind.
