# Contributing to Janus

Thank you for your interest in contributing to Janus! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Coding Standards](#coding-standards)
- [Testing Guidelines](#testing-guidelines)
- [Pull Request Process](#pull-request-process)
- [Reporting Bugs](#reporting-bugs)
- [Feature Requests](#feature-requests)
- [Documentation](#documentation)

## Code of Conduct

We are committed to providing a welcoming and inclusive environment for all contributors. Please be respectful and considerate in all interactions.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/yourusername/janus.git
   cd janus
   ```
3. **Add the upstream remote**:
   ```bash
   git remote add upstream https://github.com/original/janus.git
   ```

## Development Setup

### Prerequisites

- Rust 1.70.0 or later
- Cargo (comes with Rust)
- Docker (optional, for integration tests)

### Installation

1. Install Rust from [rustup.rs](https://rustup.rs/):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install additional tools:
   ```bash
   rustup component add rustfmt clippy
   cargo install cargo-audit cargo-watch
   ```

3. Build the project:
   ```bash
   cargo build
   ```

4. Run tests:
   ```bash
   cargo test
   ```

### Development Workflow

For continuous testing during development:
```bash
cargo watch -x check -x test -x run
```

## How to Contribute

### Types of Contributions

We welcome various types of contributions:

- **Bug fixes**: Fix issues reported in the issue tracker
- **Features**: Implement new functionality
- **Documentation**: Improve or add documentation
- **Tests**: Add or improve test coverage
- **Performance**: Optimize existing code
- **Examples**: Create examples demonstrating usage

### Before You Start

1. Check the [issue tracker](https://github.com/yourusername/janus/issues) for existing issues
2. For major changes, open an issue first to discuss your approach
3. Make sure your idea aligns with the project's goals

## Coding Standards

### Rust Style Guide

We follow the official [Rust Style Guide](https://rust-lang.github.io/api-guidelines/). Key points:

1. **Formatting**: Use `rustfmt` for code formatting
   ```bash
   cargo fmt
   ```

2. **Linting**: Use `clippy` for linting
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

3. **Naming Conventions**:
   - Use `snake_case` for functions, variables, and modules
   - Use `PascalCase` for types and traits
   - Use `SCREAMING_SNAKE_CASE` for constants

4. **Documentation**:
   - Add doc comments (`///`) for all public APIs
   - Include examples in doc comments where appropriate
   - Use `//!` for module-level documentation

### Code Quality

- Write idiomatic Rust code
- Prefer immutability where possible
- Use meaningful variable and function names
- Keep functions small and focused
- Avoid unnecessary complexity
- Handle errors appropriately (avoid unwrap in library code)

### Example

```rust
/// Processes an RDF triple and returns the subject.
///
/// # Arguments
///
/// * `triple` - The RDF triple to process
///
/// # Returns
///
/// The subject of the triple as a string
///
/// # Examples
///
/// ```
/// use janus::process_triple;
///
/// let subject = process_triple(triple)?;
/// assert_eq!(subject, "http://example.org/subject");
/// ```
///
/// # Errors
///
/// Returns an error if the triple is malformed.
pub fn process_triple(triple: &Triple) -> Result<String> {
    triple.subject()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Query("Invalid triple".to_string()))
}
```

## Testing Guidelines

### Writing Tests

1. **Unit Tests**: Place in the same file as the code being tested
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_function_name() {
           // Test code here
       }
   }
   ```

2. **Integration Tests**: Place in the `tests/` directory
   ```rust
   use janus::*;

   #[test]
   fn test_integration() {
       // Integration test code
   }
   ```

3. **Doc Tests**: Include examples in documentation
   ```rust
   /// ```
   /// use janus::function;
   /// assert_eq!(function(), expected);
   /// ```
   ```

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run integration tests
cargo test --test '*'

# Run with coverage
cargo llvm-cov --html
```

### Test Coverage

- Aim for at least 80% code coverage
- All public APIs must have tests
- Include both positive and negative test cases
- Test edge cases and error conditions

## Pull Request Process

### Before Submitting

1. **Create a new branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following the coding standards

3. **Run the full test suite**:
   ```bash
   cargo test --all-features
   cargo clippy --all-targets --all-features
   cargo fmt --all -- --check
   ```

4. **Update documentation** if needed

5. **Commit your changes** with clear, descriptive messages:
   ```bash
   git commit -m "Add feature X that does Y"
   ```

### Commit Message Guidelines

- Use the present tense ("Add feature" not "Added feature")
- Use the imperative mood ("Move cursor to..." not "Moves cursor to...")
- Limit the first line to 72 characters or less
- Reference issues and pull requests where appropriate
- Use conventional commits format:
  - `feat:` for new features
  - `fix:` for bug fixes
  - `docs:` for documentation changes
  - `test:` for test additions/changes
  - `refactor:` for code refactoring
  - `perf:` for performance improvements
  - `chore:` for maintenance tasks

Example:
```
feat: add support for MQTT stream sources

- Implement MQTT client integration
- Add configuration options for broker connection
- Include tests for MQTT functionality

Closes #123
```

### Submitting the PR

1. **Push to your fork**:
   ```bash
   git push origin feature/your-feature-name
   ```

2. **Create a Pull Request** on GitHub

3. **Fill out the PR template** with:
   - Description of changes
   - Related issue numbers
   - Testing performed
   - Breaking changes (if any)

4. **Wait for review** and address feedback

### PR Review Process

- Maintainers will review your PR
- Automated checks must pass (CI/CD)
- At least one maintainer approval is required
- Address review comments promptly
- Keep PR scope focused and manageable
- Squash commits if requested

## Reporting Bugs

### Before Reporting

1. Check if the bug has already been reported
2. Verify it's reproducible with the latest version
3. Collect relevant information

### Bug Report Template

Create an issue with the following information:

```markdown
**Description**
A clear description of the bug.

**Steps to Reproduce**
1. Step one
2. Step two
3. ...

**Expected Behavior**
What you expected to happen.

**Actual Behavior**
What actually happened.

**Environment**
- OS: [e.g., Ubuntu 22.04]
- Rust version: [e.g., 1.70.0]
- Janus version: [e.g., 0.1.0]

**Additional Context**
Any other relevant information, logs, or screenshots.
```

## Feature Requests

We welcome feature requests! Please:

1. Check if the feature has already been requested
2. Provide a clear use case
3. Explain the expected behavior
4. Consider implementation challenges
5. Be open to discussion and alternatives

## Documentation

### Types of Documentation

1. **Code Documentation**: Doc comments in source code
2. **API Documentation**: Generated from doc comments
3. **User Guide**: High-level usage documentation
4. **Architecture**: Design decisions and system overview
5. **Examples**: Practical usage examples

### Writing Documentation

- Use clear, concise language
- Include examples where helpful
- Keep documentation up-to-date with code changes
- Use proper markdown formatting
- Link to related documentation

### Building Documentation

```bash
# Build and open documentation
cargo doc --open --no-deps

# Build with all features
cargo doc --all-features --open
```

## Questions?

If you have questions about contributing, feel free to:

- Open an issue with the "question" label
- Contact the maintainers at [mailkushbisen@gmail.com](mailto:mailkushbisen@gmail.com)
- Check existing issues and discussions

## License

By contributing to Janus, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing to Janus!