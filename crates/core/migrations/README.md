# The DDL task registry (R-2026-09-24-21)

One file per task that changed `Ledger::DDL`. Each file names the task, where
it sits in the order the tasks landed, the objects it added, and the commit and
DDL digest its migration test pinned.

Before this registry existed, every migration test carried its own hand-written
`ADDED_BY_LATER_TASKS` list of the tables later tasks had added. Five lists of
the same facts, each maintained by hand, each able to drift from the others and
from the schema. A task that added a table and forgot one list turned that
list's test red for the wrong reason; a task that forgot *every* list would have
gone unnoticed by name. The registry replaces the five lists with one directory
read: `crates/core/tests/migration_registry/mod.rs` scans this directory and
yields, for any task, the set of objects tasks after it added.

**Adding a DDL task means adding a file here.** If you do not,
`crates/core/tests/migration_registry_guard.rs` goes red and names the table
nobody claimed.

## Fields

| field | meaning |
| --- | --- |
| `task` | the task card's ID, exactly as its migration test's header says it |
| `order` | where the task sits; must equal the filename's numeric prefix, and no two files may share one |
| `built_on_commit` | the commit the task was built on, as its migration test pins it; empty when the task has no migration test of its own |
| `pinned_schema_digest` | the pre-task DDL digest that test pins; empty for the same reason |
| `migration_test` | the file that is this task's compatibility evidence; empty when that evidence is still only the other tests' `added` assertions |
| `tables`, `indexes`, `triggers` | the objects the task added, derived from the schema fixtures' diffs, not transcribed from memory |
| `[[renames]]` | `kind` (`table`/`index`/`trigger`), `from`, `to` — one object a task RENAMED. A rename adds nothing, so `tables_added_after` ignores it, and a file may carry renames and no tables at all |
| `note` | why the table exists, in one or two lines |

`built_on_commit`, `pinned_schema_digest` and `migration_test` being empty is a
debt the registry makes visible rather than hides: two tasks (T-3-4, T-3-6-F1)
added tables without a migration test of their own, and this directory now says
so by name in one place instead of in a comment inside somebody else's test.


## Renames (R-2026-09-24-22)

A rename is neither an addition nor a removal, and recording it as either
would make a later-task list wrong. It gets its own file with `tables = []`
and one or more `[[renames]]`, and the guard checks it both ways: the new name
is in this tree's DDL, and **the old one is not**. The tables the renamed
object belongs to stay claimed by the task that added them, named as this tree
now creates them — so `090-t-5-3.toml` says `change_obligation_waivers` and
`100-r-2026-09-24-22.toml` is the only place in the registry where
`change_dispositions` still appears.
