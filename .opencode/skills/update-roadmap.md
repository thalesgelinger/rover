---
name: update-roadmap
summary: Update ROADMAP.md from repo state
---

Goal: keep `ROADMAP.md` aligned with docs, examples, PRs, milestones.

Steps:
1. Scan `docs/docs` for current docs
2. Scan `examples` for real usage
3. Scan runtime APIs in `rover-*` crates
4. Pull open PRs + milestones via `gh`
5. Update `ROADMAP.md` (TODO/DOING/DONE)
6. Patch docs if usage + docs diverge

Outputs:
- Updated `ROADMAP.md`
- Doc fixes/additions as needed
