# CI/CD Pipeline Quick Reference Guide

## Overview

The Janus project uses GitHub Actions for continuous integration and deployment. This guide provides a quick reference for developers working with the CI/CD pipeline.

## Pipeline Triggers

### Automatic Triggers

- **Push to branches:** `main`, `develop`, `adapter-querying`
- **Pull requests to:** `main`, `develop`
- **Release tags:** `v*` (e.g., `v1.0.0`)

### Manual Trigger

```bash
# Via GitHub UI: Actions tab → CI/CD Pipeline → Run workflow
```

## Jobs Overview

### 1. Lint (`lint`)

**Purpose:** Validates code style and formatting

**Steps:**
- Lints TypeScript with ESLint
- Checks code formatting with Prettier

**Run locally:**
```bash
npm run lint
npm run format:check
```

**Fix issues:**
```bash
npm run lint:fix
npm run format
```

### 2. Test (`test`)

**Purpose:** Runs all tests with live RDF store services

**Matrix:** Node.js 18.x and 20.x

**Services:**
- Oxigraph (port 7878)
- Jena Fuseki (port 3030)

**Environment variables:**
```yaml
OXIGRAPH_ENDPOINT: http://localhost:7878
JENA_ENDPOINT: http://localhost:3030
JENA_DATASET: ds
```

**Run locally:**
```bash
# Start services with Docker
docker run -d -p 7878:7878 --name oxigraph oxigraph/oxigraph
docker run -d -p 3030:3030 --name fuseki stain/jena-fuseki

# Run tests
npm test
```

**Features:**
- Service health checks with retry logic
- Automatic dataset creation for Fuseki
- Code coverage reporting (Node 18.x only)
- Codecov integration

### 3. Build (`build`)

**Purpose:** Compiles TypeScript to JavaScript

**Depends on:** `lint`, `test`

**Output:** `dist/` directory

**Run locally:**
```bash
npm run build
```

**Artifacts:**
- TypeScript declarations (`.d.ts`)
- Compiled JavaScript (`.js`)
- Source maps (`.js.map`)

### 4. Security (`security`)

**Purpose:** Audits npm dependencies for vulnerabilities

**Audit level:** `moderate` and above

**Run locally:**
```bash
npm audit
```

**Note:** Continues on error to not block pipeline

### 5. Dependency Review (`dependency-review`)

**Purpose:** Reviews dependency changes in pull requests

**Trigger:** Only on pull requests

**Checks:**
- New dependencies
- Updated dependencies
- License compatibility
- Known vulnerabilities

### 6. Publish (`publish`)

**Purpose:** Publishes package to npm

**Trigger:** Only on release tags (`v*`)

**Depends on:** `lint`, `test`, `build`, `security`

**Secrets required:**
- `NPM_TOKEN` - npm authentication token
- `GITHUB_TOKEN` - GitHub authentication (auto-provided)

**Process:**
1. Builds the project
2. Publishes to npm registry
3. Creates GitHub release with auto-generated notes

## Environment Setup

### Required

- **Node.js:** 18.x or higher
- **npm:** 9.x or higher

### Optional (for local testing)

- **Docker:** For running RDF store services

## Common Tasks

### Running CI Locally

```bash
# Install dependencies
npm ci

# Run linting
npm run lint

# Run tests (requires Docker services)
docker-compose up -d  # if you have docker-compose.yml
npm test

# Build project
npm run build

# Security audit
npm audit
```

### Fixing Failed Builds

#### Lint Failures

```bash
# Auto-fix ESLint issues
npm run lint:fix

# Auto-fix formatting
npm run format
```

#### Test Failures

```bash
# Run tests with verbose output
npm test -- --verbose

# Run specific test file
npm test -- path/to/test.ts

# Run tests in watch mode
npm run test:watch
```

#### Build Failures

```bash
# Clean and rebuild
npm run clean
npm run build

# Check TypeScript errors
npx tsc --noEmit
```

## Performance Metrics

### Expected Pipeline Duration

- **PR (all jobs):** 5-7 minutes
- **Main branch push:** 5-7 minutes
- **Release publish:** 8-10 minutes

### Job-Specific Times

| Job                | Duration    |
| ------------------ | ----------- |
| Lint               | 1-2 min     |
| Test (per version) | 3-4 min     |
| Build              | 1-2 min     |
| Security           | 1-2 min     |
| Dependency Review  | 1-2 min     |
| Publish            | 2-3 min     |

## Troubleshooting

### Services Not Ready

If tests fail because services aren't ready:

```yaml
# The pipeline waits up to 60 seconds for each service
# Check service logs in GitHub Actions UI
```

### Dataset Creation Fails

If Fuseki dataset creation fails:

```bash
# The pipeline continues anyway (|| true)
# Dataset might already exist from previous run
```

### Artifact Upload Fails

```bash
# Check that dist/ directory exists after build
ls -la dist/

# Verify build completed successfully
npm run build
```

### npm Publish Fails

Common issues:
1. Version already published
2. Invalid NPM_TOKEN secret
3. Package name conflict

## Best Practices

### For Contributors

1. **Run locally first:**
   ```bash
   npm run lint
   npm test
   npm run build
   ```

2. **Keep PRs focused:** Smaller PRs run faster

3. **Check CI status:** Fix issues before requesting review

4. **Use descriptive commits:** Helps with release notes

### For Maintainers

1. **Monitor pipeline performance:** Track job durations

2. **Update dependencies regularly:** Run `npm update`

3. **Review security alerts:** Check Dependabot PRs

4. **Test before releasing:** Verify builds on staging branch

## Release Process

### Creating a Release

1. **Update version:**
   ```bash
   npm version major|minor|patch
   ```

2. **Push with tags:**
   ```bash
   git push origin main --tags
   ```

3. **Monitor pipeline:** Check GitHub Actions

4. **Verify publish:** Check npmjs.com

### Versioning

- **Major (x.0.0):** Breaking changes
- **Minor (0.x.0):** New features, backward compatible
- **Patch (0.0.x):** Bug fixes

## CI/CD Configuration Files

### Main Configuration

- `.github/workflows/ci.yml` - Main pipeline

### Supporting Files

- `package.json` - Scripts and dependencies
- `tsconfig.json` - TypeScript configuration
- `jest.config.js` - Test configuration
- `.eslintrc.json` - Linting rules
- `.prettierrc.json` - Formatting rules

## Getting Help

### Pipeline Issues

1. Check GitHub Actions logs
2. Reproduce locally with same Node.js version
3. Check recent commits for breaking changes
4. Create issue with logs and error messages

### Questions

- Review this guide
- Check `RUST_REMOVAL_SUMMARY.md`
- Check `CI_CD_COMPARISON.md`
- Ask in project discussions/issues

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [npm Publishing Guide](https://docs.npmjs.com/cli/v9/commands/npm-publish)
- [Semantic Versioning](https://semver.org/)
- [Conventional Commits](https://www.conventionalcommits.org/)

## Changelog

### 2024-10-28

- Removed Rust/WASM compilation steps
- Added Oxigraph and Jena Fuseki services to test job
- Simplified from 9 jobs to 5-6 jobs
- Reduced pipeline time by ~70%
- Added dependency review for PRs
- Updated publish job to remove Rust dependencies