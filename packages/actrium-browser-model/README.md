# Browser Model

Reserved package boundary for shared browser state, workspace, tab, history,
bookmark, and settings models.

Current model code lives in the platform apps, `packages/actrium/`, and
Rust contracts under `crates/actrium-core`. Move code here only when it becomes a
stable JavaScript/TypeScript model shared by multiple app shells.
