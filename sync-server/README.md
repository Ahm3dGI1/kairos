# /sync-server

Optional, self-hostable sync backend. Sync is a layer on top of local-first storage, never a requirement: every client is fully functional offline and reconciles when back online.

Design constraints:
- Users run their own instance. No centrally-hosted service, no accounts managed by the project.
- v1 auth is a shared API key/secret between a device and its own instance — no OAuth.
- Implementation candidates: Supabase-style Postgres + realtime, or a custom minimal server.

Conflict-resolution strategy is still open (`docs/todo-app-spec.md` §6). No implementation yet.
