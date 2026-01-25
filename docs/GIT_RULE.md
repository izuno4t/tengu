# Git Rules

## Purpose

This document defines Git naming and workflow rules for this repository.
Follow these rules for all Git operations.

## Branch Naming

- Use the format `type/short-description` in lowercase (example:
  `feature/add-chrome-cookie-extractor`).
- The name must clearly convey the intent of the work on the branch.
- The short description must start with a verb and include scope when possible
  (e.g. `linux`, `windows`, `macos`).
- Avoid vague nouns and ambiguous abbreviations.
- Examples: `feature/add-linux-chrome-cookie-extractor`, `fix/windows-cookie-path`.
- Allowed types: `feature`, `fix`, `chore`, `docs`, `refactor`, `test`, `ci`.
- Use hyphens for words; avoid spaces and special characters.
- Names must be valid Git refs (no `..`, `~`, `^`, `:`, `?`, `*`, `[`, `\`,
  or trailing `.lock`).

## Commit Messages

- Use Conventional Commits: `type(scope): summary` (example:
  `feat(cli): add proxy-user option`).
- Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`.
- Keep summary in present tense and under 72 characters.

## Pull Requests

- Open a PR from a topic branch and describe what changed and why.
- Link related task IDs from `docs/TASK-1.1.0.md` when applicable.

## Sources

- Official Git ref format rules (official):
  <https://git-scm.com/docs/git-check-ref-format>
- Conventional Commits specification (official):
  <https://www.conventionalcommits.org/en/v1.0.0/>
- GitHub Docs on branches (official):
  <https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/about-branches>
