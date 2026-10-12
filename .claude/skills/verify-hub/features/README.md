# Feature map

| Feature | File | API | Page |
|---|---|---|---|
| A repo's files and checks | [repo-files-and-lint.md](repo-files-and-lint.md) | `/v1/repos/{o}/{n}`, `/tree`, `/file/{path}` | `/repos/{o}/{n}` |
| Writer projects as synthetic repos | [synthetic-repos.md](synthetic-repos.md) | same, `?source=true` | `/repos/{o}/{n}` |
| Proofreading (sous-chef) | [proofreading.md](proofreading.md) | `/v1/repos/{o}/{n}/proof` | repo page, Proofreading |
| Languages, Primary and BIEL | [catalog-and-biel.md](catalog-and-biel.md) | `/v1/languages`, `/v1/languages/{ietf}`, `/v1/biel/conflicts` | `/languages/{ietf}`, `/biel` |
| Recent feed and search | [feed-and-search.md](feed-and-search.md) | `/v1/repos?…` | `/`, `/search` |
