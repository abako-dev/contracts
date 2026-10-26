# Removed proof source and artifacts

SPEC-0015 R5 removes unused source and compiled proof wiring from the current
branch. Git history, frozen compatible predecessor executables and recovery
codecs remain. This changes no deployed instance or historical database record.

| Removed component | Last containing source | Artifact and build-lock provenance |
|---|---|---|
| calendar, ratings, projects | [canonical a0c881f](https://github.com/abako-dev/contracts/tree/a0c881fba4aee54af5aebd37e75a429838b7e8d0) | [backend manifest at8e5cece](https://github.com/abako-project/backend-v2/blob/8e5ceceba208e8577e86f52d25ac905d0d256d0e/tools/ink-sandbox/fixtures/manifest.json), [locks and artifacts](https://github.com/abako-project/backend-v2/tree/8e5ceceba208e8577e86f52d25ac905d0d256d0e/tools/ink-sandbox/fixtures) |
| storage-v5 | [canonical a0c881f](https://github.com/abako-dev/contracts/tree/a0c881fba4aee54af5aebd37e75a429838b7e8d0/storage-v5) | Library only; no standalone deployed contract or artifact |

Current calendar-v5, directory-v5 and projects-v5 use ordinary ink storage.
The old Calendar-only signed bridge and M0 business proof are replaced by the
current Directory/Project/Calendar signed suites. Unique low-level behavior stays
in the sandbox-probe tests. The paired backend's R5 inventory maps the removed
checks to their surviving coverage.
