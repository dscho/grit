# t7 family — dependency grouping (2026-06-02)

In-scope non-passing files from `data/test-files.csv`, grouped by primary subsystem.
Work order: quick wins → reset/clean → status → commit → grep/repack/difftool → submodules (largest).

## Group A — Quick wins (mv, editor, setup, hooks) — ~13 failing
| File | pass/total | failing | Depends on |
|------|------------|---------|------------|
| t7001-mv | 51/54 | 3 | mv, index |
| t7005-editor | 10/11 | 1 | GIT_EDITOR, commit |
| t7008-filter-branch-null-sha1 | 5/6 | 1 | filter-branch |
| t7010-setup | 12/16 | 4 | git-sh-setup |
| t7450-bad-git-dotfiles | 49/50 | 1 | init/config |
| t7505-prepare-commit-msg-hook | 22/23 | 1 | hooks |
| t7508-status | 124/126 | 2 | status |
| t7516-commit-races | 0/2 | 2 | commit locking |
| t7527-builtin-fsmonitor | 0/0 | 0 | fsmonitor (0 tests?) |
| t7900-maintenance | 71/72 | 1 | maintenance |
| t7111-reset-table | 41/42 | 1 | reset |
| t7426-submodule-get-default-remote | 14/15 | 1 | submodule config |
| t7418-submodule-sparse-gitmodules | 8/9 | 1 | .gitmodules |

## Group B — Worktree status (t706x) — ~20 failing
| t7061-wtstatus-ignore | 13/25 | 12 | status, ignore, linked wt |
| t7064-wtstatus-pv2 | 20/28 | 8 | porcelain v2 status |

## Group C — Reset pathspec/hooks (t71xx) — ~37 failing
| t7107-reset-pathspec-file | 1/11 | 10 | reset, pathspec-from-file |
| t7112-reset-submodule | 54/78 | 24 | reset + submodule |
| t7113-post-index-change-hook | 1/4 | 3 | hooks |

## Group D — Clean interactive — 15 failing
| t7301-clean-interactive | 8/23 | 15 | clean -i |

## Group E — Commit porcelain (t75xx) — ~136 failing
| t7502-commit-porcelain | 32/82 | 50 | commit -a, porcelain |
| t7501-commit-basic-functionality | 52/77 | 25 | commit basics |
| t7507-commit-verbose | 10/45 | 35 | commit -v |
| t7500-commit-template-squash-signoff | 42/57 | 15 | template, squash |
| t7509-commit-authorship | 4/12 | 8 | author ident |
| t7514-interpret-trailers-options | 1/10 | 9 | trailers CLI |

## Group F — Status help/rename/fsmonitor (t75xx) — ~37 failing
| t7512-status-help | 20/47 | 27 | status --help |
| t7525-status-rename | 10/15 | 5 | status rename |
| t7519-status-fsmonitor | 30/33 | 3 | fsmonitor |

## Group G — Merge/repack small (t76xx/t77xx) — ~34 failing
| t7602-merge-octopus-many | 2/5 | 3 | merge |
| t7606-merge-custom | 2/4 | 2 | merge |
| t7615-diff-algo-with-mergy-operations | 5/7 | 2 | merge diff |
| t7700-repack | 40/47 | 7 | repack |
| t7701-repack-unpack-unreachable | 1/9 | 8 | repack |
| t7703-repack-geometric | 6/18 | 12 | repack |

## Group H — Grep (t781x) — ~53 failing
| t7810-grep | 223/263 | 40 | grep |
| t7814-grep-recurse-submodules | 17/27 | 10 | grep + submodule |
| t7818-grep-extended | 8/11 | 3 | grep -E |

## Group I — Difftool — 40 failing
| t7800-difftool | 55/95 | 40 | difftool |

## Group J — Submodules (t74xx + related) — ~200+ failing
| t7406-submodule-update | 10/70 | 60 | submodule update |
| t7403-submodule-sync | 1/18 | 17 | submodule sync |
| t7407-submodule-foreach | 4/23 | 19 | submodule foreach |
| t7400-submodule-basic | 96/124 | 28 | submodule basics |
| t7401-submodule-summary | 10/25 | 15 | submodule summary |
| t7506-status-submodule | 20/40 | 20 | status + submodule |
| (+ many more t742x with smaller counts) |

## Completed this sprint (Group A partial)
- t7005-editor: `git var GIT_EDITOR` default `vi` when TERM is not dumb (removed erroneous stdin TTY check).
- t7008-filter-branch-null-sha1: resolve `git-filter-branch` script when harness `GIT_EXEC_PATH` lacks it.
- t7508-status: global `--no-optional-locks` sets `GIT_OPTIONAL_LOCKS=0`.
- maintenance: `pack_loose` progress line for `GIT_PROGRESS_DELAY` (t7900 test 24 still under investigation).

## Current sprint: Group A (remaining quick wins)
