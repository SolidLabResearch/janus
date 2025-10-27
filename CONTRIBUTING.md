# Contributing to Janus RDF Template

Thank you for your interest in contributing to Janus! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Development Workflow](#development-workflow)
- [Coding Standards](#coding-standards)
- [Testing Guidelines](#testing-guidelines)
- [Commit Message Format](#commit-message-format)
- [Pull Request Process](#pull-request-process)
- [Reporting Issues](#reporting-issues)

## Code of Conduct

This project adheres to a Code of Conduct that all contributors are expected to follow. Please be respectful and constructive in all interactions.

### Our Standards

- Be welcoming and inclusive
- Respect differing viewpoints and experiences
- Accept constructive criticism gracefully
- Focus on what is best for the community
- Show empathy towards other community members

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/janus.git
   cd janus
   ```
3. Add upstream remote:
   ```bash
   git remote add upstream https://github.com/ORIGINAL_OWNER/janus.git
   ```
4. Create a branch for your work:
   ```bash
   git checkout -b feature/my-new-feature
   ```

## Development Setup

### Prerequisites

- Node.js >= 18.0.0
- npm >= 9.0.0
- Rust >= 1.70.0
- wasm-pack

### Installation

```bash
# Install Node dependencies
npm install

# Install Rust dependencies and build
cd rust
cargo build
cd ..

# Build WASM modules
npm run build:rust
```

### Running in Development Mode

```bash
# Watch TypeScript files
npm run dev

# Run tests in watch mode
npm run test:watch
```

## Project Structure

```
janus/
├── src/                   # TypeScript source
│   ├── core/             # Core functionality
│   ├── adapters/         # Store adapters
│   └── utils/            # Utilities
├── rust/                 # Rust source
│   ├── src/
│   │   ├── lib.rs       # Library entry
│   │   ├── store.rs     # Store implementation
│   │   ├── query.rs     # Query execution
│   │   └── ...
│   └── tests/           # Rust tests
├── tests/               # TypeScript tests
└── examples/            # Usage examples
```

## Development Workflow

### 1. Sync with Upstream

Before starting work, sync your fork:

```bash
git fetch upstream
git checkout main
git merge upstream/main
git push origin main
```

### 2. Create a Feature Branch

```bash
git checkout -b feature/description-of-feature
```

Branch naming conventions:
- `feature/` - New features
- `fix/` - Bug fixes
- `docs/` - Documentation changes
- `refactor/` - Code refactoring
- `test/` - Test additions or modifications
- `chore/` - Maintenance tasks

### 3. Make Your Changes

- Write clean, readable code
- Follow the coding standards
- Add tests for new functionality
- Update documentation as needed

### 4. Test Your Changes

```bash
# Run all tests
npm test

# Run linting
npm run lint

# Run formatting
npm run format
```

### 5. Commit Your Changes

Follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```bash
git add .
git commit -m "feat: add new RDF parser for JSON-LD"
```

### 6. Push and Create Pull Request

```bash
git push origin feature/description-of-feature
```

Then create a Pull Request on GitHub.

## Coding Standards

### TypeScript

- Use TypeScript strict mode
- Prefer `const` over `let`, avoid `var`
- Use meaningful variable and function names
- Document public APIs with JSDoc comments
- Keep functions small and focused
- Use async/await over promises chains

Example:

```typescript
/**
 * Execute a SPARQL query against the RDF store
 * @param sparql - The SPARQL query string
 * @param options - Optional query execution options
 * @returns Query results in JSON format
 */
async function executeQuery(
  sparql: string,
  options?: QueryOptions
): Promise<QueryResult> {
  // Implementation
}
```

### Rust

- Follow Rust naming conventions (snake_case for functions/variables)
- Use `Result<T, E>` for error handling
- Document public APIs with doc comments
- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Prefer iterators over loops

Example:

```rust
/// Execute a SPARQL query and return results as JSON
///
/// # Arguments
/// * `query` - The SPARQL query string
///
/// # Returns
/// * `Result<String, RdfError>` - JSON string or error
pub fn execute_query(&self, query: &str) -> Result<String, RdfError> {
    // Implementation
}
```

### General Guidelines

- DRY: Don't Repeat Yourself
- KISS: Keep It Simple, Stupid
- YAGNI: You Aren't Gonna Need It
- Single Responsibility: Each module/function should have one clear purpose
- Consistent Naming: Use consistent naming patterns throughout

## Testing Guidelines

### TypeScript Tests

Use Jest for TypeScript testing:

```typescript
import { OxigraphAdapter } from '../src/adapters/OxigraphAdapter';
import { RdfFormat } from '../src/core/types';

describe('OxigraphAdapter', () => {
  let adapter: OxigraphAdapter;

  beforeEach(() => {
    adapter = new OxigraphAdapter({
      url: 'http://localhost:7878',
      storeType: 'oxigraph',
    });
  });

  afterEach(async () => {
    await adapter.clear();
  });

  it('should load RDF data successfully', async () => {
    const turtle = testUtils.createSampleTurtle();
    const count = await adapter.loadData(turtle, RdfFormat.Turtle);
    expect(count).toBeGreaterThan(0);
  });

  it('should execute SPARQL SELECT query', async () => {
    const result = await adapter.query('SELECT * WHERE { ?s ?p ?o }');
    expect(result).toHaveProperty('results');
  });
});
```

### Rust Tests

Use built-in Rust testing:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_creation() {
        let store = RdfStore::new();
        assert!(store.is_ok());
    }

    #[test]
    fn test_load_and_query() {
        let mut store = RdfStore::new().unwrap();
        let data = r#"
            @prefix ex: <http://example.org/> .
            ex:Alice ex:knows ex:Bob .
        "#;
        
        let count = store.load_data(data, RdfFormat::Turtle, None).unwrap();
        assert_eq!(count, 1);
        
        let result = store.query("SELECT * WHERE { ?s ?p ?o }");
        assert!(result.is_ok());
    }
}
```

### Test Coverage

- Aim for >70% code coverage
- Test edge cases and error conditions
- Include integration tests for adapters
- Mock external dependencies when appropriate

## Commit Message Format

Follow [Conventional Commits](https://www