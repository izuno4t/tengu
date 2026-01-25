# Contributing Guide

Other languages: [日本語版](CONTRIBUTING.ja.md)

Thank you for considering contributing to Tengu! 👺

## Development Environment Setup

### Requirements

- Rust 1.75 or later
- Cargo

### Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/tengu.git
cd tengu

# Build
cargo build

# Run tests
cargo test

# Run
cargo run
```

## Coding Standards

### Rust

- Use Rust 2021 edition
- Follow all `cargo clippy` recommendations
- Format with `cargo fmt`
- Add doc comments for all public functions

### Commit Messages

Conventional Commits format is recommended:

```text
feat: add new feature
fix: bug fix
docs: documentation update
refactor: refactor code
test: add or update tests
chore: other changes
```

Examples:

```text
feat: add MCP server auto-discovery
fix: fix crash when saving session
docs: add installation steps to README.md
```

## Pull Request Process

1. **Create an Issue**
   - For new features or major changes, open an Issue first to discuss

2. **Create a Branch**

   ```bash
   git checkout -b feature/awesome-feature
   ```

3. **Development**
   - Write tests
   - Update documentation
   - Check with `cargo clippy`
   - Format with `cargo fmt`

4. **Commit**

   ```bash
   git commit -m "feat: Add awesome feature"
   ```

5. **Push**

   ```bash
   git push origin feature/awesome-feature
   ```

6. **Open a Pull Request**
   - Clearly describe the changes
   - Link related Issues

## Tests

### Unit Tests

```bash
cargo test
```

### Run a Specific Test

```bash
cargo test test_name
```

### Coverage

```bash
cargo tarpaulin --out Html
```

## Documentation

### Generate API Docs

```bash
cargo doc --open
```

### README Updates

If you add new features, please update README.md as well.

## Release Process

1. Update the version in `Cargo.toml`
2. Update the CHANGELOG
3. Create a tag

   ```bash
   git tag -a v0.2.0 -m "Release v0.2.0"
   git push origin v0.2.0
   ```

## Questions & Support

- GitHub Issues: bug reports and feature requests
- GitHub Discussions: questions and discussions

## Code of Conduct

- Aim for constructive feedback
- Respect other contributors
- Maintain an open and welcoming community

## License

Contributions are provided under the MIT License.
