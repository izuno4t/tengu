# Rust Coding Guidelines for Agents

This document defines general, widely accepted Rust coding practices
for coding agents working on this repository.

These rules apply unless explicitly overridden by repository-specific instructions.

The goal is not stylistic purity,
but to prevent structural degradation, unreadable code, and unsafe assumptions.

---

## 0. Core Principles

- Prefer correctness, clarity, and explicitness over cleverness.
- Do not guess intent. If intent is unclear, stop and ask.
- Avoid premature optimization unless performance requirements are explicit.
- Treat public APIs as long-term contracts.

---

## 1. Project and Module Structure

### 1.1 Module organization

- One module should have one clear responsibility.
- Split modules when responsibilities diverge.
- Follow existing repository layout first; do not introduce new patterns casually.

Recommended defaults:

- Small module -> src/foo.rs
- Growing module -> src/foo/mod.rs with submodules

---

## 2. Visibility and Public API Boundaries

- Everything is private by default.
- pub is a compatibility promise.
- Prefer pub(crate) when external exposure is unnecessary.
- Never make an item pub solely to satisfy tests.

---

## 3. Error Handling

### 3.1 Result and error types

- Libraries must use structured error types (enum-based).
- Avoid String errors in public APIs.
- Prefer domain-specific error variants.

### 3.2 Error propagation

- Use ? for propagation.
- Avoid deeply nested match when propagation is sufficient.
- Add context only at meaningful boundaries.

---

## 4. Ownership, Borrowing, and Data Passing

### 4.1 Function signatures

- Prefer borrowing for inputs:
  - &str over String
  - &[T] over Vec<T>
- Return owned values when it improves API ergonomics.

### 4.2 Cloning

- Do not use .clone() just to satisfy the borrow checker.
- If cloning is intentional, document the reason briefly.

---

## 5. Performance and Allocation

- Prefer readable code over micro-optimizations.
- Avoid allocations only when profiling or requirements justify it.
- Iterator-based code is preferred when it improves clarity.

---

## 6. Async and Concurrency

- Follow the runtime already used by the project.
- Do not introduce async or concurrency without explicit need.
- Avoid blocking operations inside async contexts.

---

## 7. Testing Guidelines

Rust supports multiple test styles. Choose intentionally.

---

### 7.1 Unit Tests (Internal)

Unit tests verify internal correctness.

They may:

- Access private items
- Test invariants and edge cases
- Validate small, focused logic

---

### 7.2 Unit Test Placement Rules (Critical)

Rust allows unit tests inside implementation files,
but this is not a mandate.

Allowed inside the same file only if all conditions apply:

- Test code is under ~20–30 lines
- Targets small, self-contained logic
- Test cases are few and unlikely to grow
- Implementation readability is not degraded

Short, local, stable tests only.

---

Tests MUST be split into separate files when any applies:

- Test code exceeds ~50 lines
- Many test cases or table-driven tests appear
- Explanatory comments increase
- Tests change independently of implementation
- Implementation reading flow is disrupted

If it feels heavy, split immediately.

---

### 7.3 Recommended Structure for Split Unit Tests

src/
 ├── foo.rs
 └── foo/
     └── tests.rs

Characteristics:

- Keeps tests logically close to implementation
- Prevents file bloat
- Preserves access to internal items
- Maintains clear separation from integration tests

---

### 7.4 Integration Tests (tests/ directory)

Integration tests verify public behavior.

Use integration tests when:

- Testing public APIs
- Verifying multi-module interactions
- Representing user-facing scenarios
- Involving I/O, configuration, or environment

Integration tests must use only public APIs.

---

### 7.5 Prohibited Practices

- Writing all tests inline by default
- Letting implementation files grow indefinitely
- Making items pub only for testing
- Mixing unit and integration tests in one place

---

## 8. Documentation and Comments

- Use Rust doc comments (///) for public items.
- Document:
  - What it does
  - Constraints and invariants
  - Error conditions
- Avoid long narrative comments inside functions.
  Refactor instead.

---

## 9. Formatting and Linting

### 9.1 Formatting

- Always format with cargo fmt.

### 9.2 Linting (Clippy)

- Use Clippy as guidance, not authority.
- Do not distort APIs to satisfy lints.
- When a lint conflicts with intentional design:
  - Use a narrow #[allow(clippy::...)]
  - Add a brief justification

Treating warnings as errors (-D warnings) is strict
and may cause breakage on toolchain updates.

---

## 10. Dependencies and Features

- Do not add dependencies without justification.
- Prefer mature, widely used crates.
- Avoid enabling heavy default features unnecessarily.
- Keep feature flags minimal and explicit.

---

## 11. Security and Sensitive Data

- Never log credentials, tokens, cookies, or secrets.
- Treat local-user data as sensitive.
- Require explicit opt-in for dangerous operations.
- Avoid export-everything style APIs.

---

## 12. Definition of Done (Rust)

A task is complete only when:

- cargo build succeeds
- cargo test passes
- cargo fmt is clean
- Lints are acceptable per project policy
- Public API changes are documented
- Task status is updated

---

## Appendix: Common Commands

- Build: cargo build
- Test: cargo test
- Format: cargo fmt
- Lint: cargo clippy --lib --tests
- Coverage (if enabled):
  cargo llvm-cov --codecov --output-path codecov.json
