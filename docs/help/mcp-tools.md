# MCP Tools Reference

Detailed list of MCP tools (methods) with payloads.

- task/create(TaskCreate) -> { task: TaskDTO }
- task/get({ id, project? }) -> { task: TaskDTO }
- task/update({ id, patch: TaskUpdate }) -> { task: TaskDTO }
- task/delete({ id, project? }) -> { deleted: bool }
- task/list(TaskListFilter) -> { tasks: TaskDTO[] }
- project/list({}) -> { projects: ProjectDTO[] }
- project/stats({ name }) -> { stats: ProjectStatsDTO }
- config/show({ global?, project? }) -> { config: object }
- config/set({ global?, project?, values }) -> { updated: bool }

Notes
- snake_case keys; enums match CLI/REST.
- People fields accept `@me` (identity rules apply).

Examples
```json
{"jsonrpc":"2.0","id":1,"method":"task/create","params":{"title":"X","project":"DEMO"}}
```

See also: [Identity & Users](./identity.md) and [Task Model](./task-model.md).
