# Devlog 0004 — Pop-up fixes, 0.1.1

- **Date:** 2026-11-01
- **Reported by KJ** from the released 0.1.0: a see-through box around the pop-up, a close button that did
  nothing, a spinning cursor, and "Wants permission to use AskUserQuestion".
- **Causes and fixes:**
  - **Close:** the pop-up's own click handlers closed it through its window handle. gpui can't reach a window
    while it is handling one of that window's events, so the close silently failed and Samo lost track of the
    pop-up. The handlers now close it through the window they're given.
  - **Box:** the window was the card plus a 16 pt margin for a shadow Samo drew itself. The shadow was larger
    than the margin, so the window's edge cut it into a box. The window is now exactly the card, and macOS draws
    the shadow.
  - **Wording:** Claude Code sends `PermissionRequest` for its own `AskUserQuestion` and `ExitPlanMode` tools.
    These now read as a question and a plan to review.
  - **The spinning cursor** didn't happen again after the close fix: hovering and clicking stayed responsive.
- **Verification:** `scripts/ci-local.sh` (168 tests). Live check: a pop-up from a real hook, held under the
  pointer, then closed with one click on ×; no box; Samo responsive throughout.
- **Versions:** Samo 0.1.1, Claude Sessions 0.1.1.
