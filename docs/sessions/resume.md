# Resume Orchestrator (Self-Healing)

Use this prompt when the orchestrator session's context exploded or was interrupted.
Copy everything below `---` into a new Claude Code session in `/home/cy/Code/MT5/nautilus_trader/.claude/worktrees/sinopac-stop-orders`.

---

You are the development orchestrator for the NautilusTrader Sinopac stop-orders feature. **Your previous session was interrupted.** Resume from where it left off.

## Recovery Steps

1. **Read progress state:**
   Read `docs/sessions/progress.json`. Note `model_assignments`. To re-dispatch a role:
   (1) If `.claude/agents/orchestrator-<role>.md` exists, dispatch with
       `subagent_type: "orchestrator-<role>"` — model and effort are enforced by its
       frontmatter.
   (2) If the agent file is missing but `model_assignments` has the role, dispatch with
       the Agent tool's `model` parameter and inline the role content from the standalone
       role file (`docs/sessions/01-executor.md`, `02-code-reviewer.md`, or
       `03-qa-tester.md`); effort cannot be enforced in this mode.
   (3) If neither exists, dispatch with no `model` parameter — the session defaults.
   NOTE: `.claude/agents/` is gitignored in this repo, so the agent files exist on disk in
   this worktree but are not version-controlled — they resolve as subagent types here.

2. **Read the orchestrator rules:**
   Read `docs/sessions/orchestrator.md` for the full pipeline flow, dispatch templates, and orchestration logic. Follow those rules exactly.

3. **Read the plans** (for context):
   - `docs/plans/2026-06-26-sinopac-stop-orders.md`
   - `docs/superpowers/specs/2026-06-26-sinopac-stop-orders-design.md`

4. **Read recent git history:**
   `git log --oneline -30`

5. **Read any existing review/QA reports:**
   `ls docs/reviews/ docs/qa/ 2>/dev/null`

6. **Determine where to resume** based on `progress.json`:

| `current_step` | What to do |
|----------------|------------|
| `execute` | Executor was in progress. Check git log — if commits exist for current batch tasks, executor may have partially finished. Dispatch executor for remaining tasks only. |
| `review` | Review was in progress or not started. Check if review report exists. If not, dispatch reviewer. If exists but incomplete, dispatch reviewer again. |
| `fix` | Fixes were in progress. Check if fix commits exist since review report. Dispatch executor for remaining Critical/Important issues. |
| `fix_verify` | Fix verification was in progress. Dispatch reviewer to verify fixes. |
| `qa` | QA was in progress. Check if QA report exists. If not, dispatch QA. |
| `qa_fix` | QA bug fixes in progress. Check QA report for remaining bugs. Dispatch executor. |
| `qa_verify` | QA re-verification. Dispatch QA to verify fixes. |
| `final_audit` | Audit was in progress. Re-invoke `/code-review` (or the reviewer-audit fallback) on the whole branch vs `sinopac-adapter-clean`. |
| `audit_fix` | Audit fixes in progress. Check the final-audit report for unresolved Critical findings; dispatch executor. |
| `audit_verify` | Re-run the audit to verify fixes (counts toward the 2-cycle cap). |
| `done` | Everything is complete. Nothing to do. |

7. **Resume the orchestration loop** from the determined point, following the rules in `docs/sessions/orchestrator.md`.

## Start

Read `docs/sessions/progress.json`, then read `docs/sessions/orchestrator.md`. Determine current state, announce where you're resuming from, and continue the pipeline.
