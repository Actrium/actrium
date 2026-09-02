# System Apps

Reserved package boundary for built-in Actrium apps such as start page, history,
bookmarks, settings, and global search.

Current implementation lives in `packages/actrium/`. Keep system app code
there until splitting it creates a cleaner package boundary without fragmenting
the React Native runtime.
