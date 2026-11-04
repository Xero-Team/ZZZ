---
title: Database
description: Browse SQLite databases, run SQL, edit rows, and inspect schemas in ZZZ.
---

# Database

ZZZ includes a database browser for local SQLite files. You can inspect the
schema, run SQL, edit rows inside an explicit transaction, review query history,
draw an ER diagram, and see how SQLite plans a query. The built-in driver is
SQLite; other drivers can be registered by extensions.

## Opening a database {#opening}

Run `database: open` from the command palette to pick a `.db` or `.sqlite` file.
The connection is saved, so you can reopen it from the Databases panel later.

- `database: toggle panel` focuses the Databases panel.
- `database: add connection` opens the connection dialog for a named connection.

Each database opens in a tab with the schema tree on one side and the SQL editor
and result grid on the other.

## Connections {#connections}

The Databases panel lists saved connections and their runtime state. State is
never persisted: a connection is disconnected until you connect it in this
session.

- Use the connect button to connect it, and the open button to open its
  browser tab.
- Use the edit action to change its name or file, test it, or delete it.
- Passwords for network drivers are stored in the system keychain. They are
  never written to `settings.json`, logs, errors, query history, or the MCP
  output.

The connection dialog creates file connections. A server connection can be
written in `settings.json` with `host`, `port`, `user`, and `database`; the
dialog then shows those fields and a password field when you edit it.

Connections can also be written directly in settings:

```json [settings]
{
  "database": {
    "connections": [
      {
        "id": "work",
        "name": "Work",
        "driver": "sqlite",
        "path": "/home/you/work.sqlite"
      }
    ]
  }
}
```

`id` is stable and is also the keychain entry name. Renaming `name` does not
change it.

## Browsing the schema {#schema}

A connection expands into tables and views. Selecting one describes it from the
database itself, not from a cached list:

- columns with declared types and nullability
- primary key, indexes, and foreign keys

`ER Diagram` renders the described tables as a Mermaid `erDiagram`. Use
`Schema CSV` or `Schema JSON` to copy the described schema to the clipboard.

## Running SQL {#queries}

Type SQL in the editor and run it:

| Action        | macOS             | Linux and Windows  |
| ------------- | ----------------- | ------------------ |
| Run statement | `Cmd-Enter`       | `Ctrl-Enter`       |
| Run selection | `Cmd-Shift-Enter` | `Ctrl-Shift-Enter` |
| Cancel query  | `Cmd-Shift-C`     | `Ctrl-Shift-C`     |

`Run statement` runs the selection when there is one, otherwise the statement
under the cursor. `Run selection` requires selected text.

A cancelled query is reported as cancelled, not as an error. Running a query
waits for the connection if one is open; a new view connects on its first run.

Results load up to 10,000 rows. When a result is larger, the grid says so and
stops reading; sorting applies to the loaded window, not to the full table. The
grid has virtual scrolling, draggable column widths, and cell copy. `Copy CSV`,
`Copy JSON`, `Copy Markdown`, and `Copy HTML` put the displayed result on the
clipboard.

`Explain` shows SQLite's `EXPLAIN QUERY PLAN` output as a tree. It is available
for drivers that declare the capability.

## Editing rows {#editing}

Editing requires a single-table `SELECT` that returns the primary key. BLOB
values are shown as a digest and cannot be edited.

1. Select a cell, type the new value, and press `Enter` to stage it. `Escape`
   cancels the cell edit.
2. Use `Apply` to write staged edits into the open transaction. Applied edits
   stay visible until you commit or roll back.
3. Use `Commit` to commit everything, or `Rollback` to discard it.

`Commit` also applies any staged edits that were not applied yet. Staged edits
that were never applied are discarded by `Rollback`; the view tells you when
there is nothing staged to discard.

The confirmation dialog shows the statement for `UPDATE` and `DELETE` without
a `WHERE` clause, and for schema changes, so you can see which object it
touches. Closing a tab with uncommitted work asks first: `Save` commits,
`Don't Save` rolls back, and `Cancel` leaves the tab open.

Running a new query while edits are pending asks you to commit or roll back
first, because a new result set cannot own the old edits.

## Query history {#history}

The `History` button lists statements this connection ran. You can search the
history and either load a statement into the editor or run it again. Each
connection keeps its newest 500 entries. The history stores the SQL, time, row
count, and duration; it does not store result rows or credentials.

## Importing CSV {#import}

`Import CSV…` appends rows from a CSV file to the selected table. The import
runs in a transaction and uses parameterized inserts. It is offered only for
drivers that support transactions and row editing. If the import fails, the
whole import is rolled back.

The header row names the target columns; unknown or missing columns are
reported instead of guessed.

## SQL notebooks {#notebooks}

`Notebook` opens a tab with cells that share the connection. Each cell has its
own session, so:

- running a cell does not block the editor or other cells
- cancelling a cell cancels only that cell's query
- each result stays under the cell that produced it

A cell previews the first 100 rows. Open the result in the database tab when you
need sorting or export. Cells can be added, moved, and deleted. Notebook contents
are not saved.

## Extension drivers {#extension-drivers}

An extension can register a database driver by declaring a `database_drivers`
entry that points at a command. ZZZ starts that command with
`smol::process::Command` and exchanges newline-delimited JSON over stdin and
stdout. The driver declares its capabilities, and the UI only offers actions
those capabilities allow.

The connection dialog has no driver selector. To use an extension driver, set
`driver` in `settings.json` to the namespaced id, for example
`my-extension.postgres`. A driver from an uninstalled extension stays registered
until the next restart.

## MCP server {#mcp}

`database-mcp-server` exposes saved SQLite connections to an agent as MCP tools:
`list_connections`, `describe_table`, and `run_read_query`. Writes require the
explicit `--allow-writes` flag. Read-only calls run with `PRAGMA query_only = 1`
on a connection that exists only for that call.

See [Database MCP server](./development/database/mcp-server.md) for the build
command, `context_servers` configuration, and the tool contract.

## Limitations {#limitations}

- The built-in driver is SQLite only.
- Database files in remote projects and SSH tunnels are not supported yet.
- Query results are bounded to the first 10,000 rows.
- BLOB values are displayed as digests and cannot be edited.
- Notebook contents are not persisted.
- Schema changes are not edited in the browser; run DDL in the SQL editor, where
  a confirmation is required.
