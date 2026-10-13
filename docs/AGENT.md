# Console agent and MCP

Keep `craft-launcher-agent.exe`, `craft-app-launcher.exe`, and `craft-launcher-pair.json` from the same release together. Use the console agent for automation. The GUI executable is the interactive launcher.

## CLI

From the launcher directory in PowerShell:

```powershell
.\craft-launcher-agent.exe state
.\craft-launcher-agent.exe profiles
.\craft-launcher-agent.exe installations
```

These reads provide the installation and profile IDs needed for explicit actions. Do not invent IDs or choose an installation on the user's behalf.

```powershell
.\craft-launcher-agent.exe launch launch-001 filmcraft INSTALL_ID PROFILE_ID gui
.\craft-launcher-agent.exe launch launch-002 filmcraft INSTALL_ID PROFILE_ID cli
.\craft-launcher-agent.exe open-apps-folder folder-001
.\craft-launcher-agent.exe update-all update-001
.\craft-launcher-agent.exe operation-status update-001
```

Replace `INSTALL_ID` and `PROFILE_ID` with returned values. Use a fresh action ID for each mutation. Update All downloads and installs eligible updates; run it only when that action is intended.

Other forms are:

```text
patch ID FIELDS_JSON
reload ID
releases-check ID APP
install-plan ID APP TAG ASSET_JSON
install-confirm ID PLAN_JSON
cancel OPERATION_ID
request [JSON]
```

For single-app installation, obtain official metadata with `releases-check`, pass the selected returned asset to `install-plan`, and explicitly confirm the returned plan with `install-confirm`. Do not fabricate asset URLs, sizes, digests, or plan bindings.

`request` without an inline argument reads up to 64 KiB from stdin. For example:

```powershell
'{"protocol":1,"id":"read-001","action":{"action":"state"}}' | .\craft-launcher-agent.exe request
```

Protocol 1 uses an envelope containing `protocol`, `id`, and a tagged `action` object. Action IDs contain 1–80 ASCII letters, digits, dots, underscores, or hyphens. Unknown fields are rejected. Responses contain `protocol`, `id`, `ok`, `code`, and `result`. CLI stdout carries one JSON response; diagnostics use stderr. Exit codes are 0 for success, 2 for an action/unavailability error, 3 for owner busy, and 4 for response-output failure.

## MCP over stdio

Configure your MCP client to start the console agent on demand:

```json
{
  "mcpServers": {
    "craft-launcher": {
      "command": "C:\\Tools\\CraftLauncher\\craft-launcher-agent.exe",
      "args": ["--mcp"]
    }
  }
}
```

Use your client's equivalent stdio configuration if its format differs. No network listener or permanently running MCP service is required. The server exposes `launcher_action` with the protocol-1 request schema. Read `state`, `profiles`, and `installations` before selecting mutations. MCP request IDs are separate from launcher action IDs.

With a GUI owner, long-running operations return a receipt/request ID that can be queried with `operation_status` and canceled with `cancel`. Headless operations run synchronously and have no durable receipt. Installation listings support `offset` and `limit` through JSON requests; the convenience CLI lists the first 32.

Never automatically replay a mutation after a timeout, disconnect, cancellation, or unknown outcome. A started action may have changed files even if its response was lost. Reconcile current state and installed folders before deciding what to do next. Cancellation does not promise rollback.

In a source checkout, action definitions are in `src/actions.rs`, CLI parsing is in `src/agent_main.rs`, and MCP behavior is in `src/mcp.rs`. Source files are not included in the portable package.
