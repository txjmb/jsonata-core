# GitHub Actions Workflows

This directory contains automated CI/CD workflows for jsonatapy. All workflows follow best practices including caching, concurrency control, minimal permissions, and comprehensive reporting.

## Workflows Overview

### 1. Test Suite (`test.yml`)
**Triggers:** Push, Pull Request, Manual
**Purpose:** Run comprehensive test suite across platforms and Python versions

- Rust unit tests (all platforms)
- Python integration tests (Python 3.9-3.12)
- Reference JSONata test suite (1258 tests)
- Cross-platform testing (Linux, macOS, Windows)

**Usage:**
```bash
# Triggered automatically on push/PR
# Or run manually from Actions tab
```

### 2. Release (`release.yml`)
**Triggers:** Manual workflow_dispatch
**Purpose:** Automated release process with version management

**Version Scheme:** `X.Y.Z.P`
- `X.Y.Z` = JSONata version (e.g., 2.0.5)
- `P` = jsonatapy patch version (0, 1, 2, etc.)

**Features:**
- ✅ Version validation and update in Cargo.toml/pyproject.toml
- ✅ Multi-platform wheel builds (Linux x86_64/aarch64, macOS x86_64/ARM64, Windows x86_64)
- ✅ Python 3.9-3.12 support
- ✅ Wheel testing before publishing
- ✅ Automated changelog generation
- ✅ PyPI publishing
- ✅ GitHub release creation
- ✅ Dry-run mode for testing

**Usage:**
```bash
# From GitHub Actions UI:
# 1. Go to Actions tab
# 2. Select "Release" workflow
# 3. Click "Run workflow"
# 4. Enter version (e.g., "2.0.5.2")
# 5. Optionally enable dry_run to test without publishing
```

**Required Secrets:**
- `PYPI_API_TOKEN` - PyPI API token for publishing

### 3. Benchmark (`benchmark.yml`)
**Triggers:** PR, Push to main, Weekly schedule, Manual
**Purpose:** Performance testing and regression detection

**Features:**
- ✅ Full benchmark suite (30+ tests)
- ✅ Comparison with baseline (main branch)
- ✅ Regression detection (>10% slower = fail)
- ✅ Automatic PR comments with results
- ✅ Performance charts generation
- ✅ Historical results tracking
- ✅ GitHub Pages publishing

**Usage:**
```bash
# Automatically runs on PR
# Manual run with full iterations:
# Actions → Benchmark → Run workflow → Enable "full_benchmark"
```

**Outputs:**
- JSON results files
- Performance charts (speedup_comparison.png, category_comparison.png)
- PR comments with benchmark summary
- Historical data on gh-pages branch

### 4. Documentation (`docs.yml`)
**Triggers:** Push to main, Releases, Manual
**Purpose:** Build and deploy documentation to GitHub Pages

**Features:**
- ✅ MkDocs with Material theme
- ✅ API documentation generation
- ✅ Compatibility matrix
- ✅ Performance benchmark integration
- ✅ Automatic deployment to GitHub Pages
- ✅ Version information

**Pages Structure:**
- Home / Getting Started
- API Reference (Python)
- Examples and tutorials
- Compatibility matrix
- Performance benchmarks
- Migration guide

**Usage:**
```bash
# Automatically builds and deploys on push to main
# View at: https://yourusername.github.io/jsonatapy/
```

**Required Settings:**
- Enable GitHub Pages in repository settings
- Set source to "GitHub Actions"

### 5. Security (`security.yml`)
**Triggers:** Push, PR, Daily schedule, Manual
**Purpose:** Comprehensive security scanning

**Features:**
- ✅ Rust dependency audit (`cargo audit`)
- ✅ Rust license/security check (`cargo deny`)
- ✅ Python dependency audit (`pip-audit`)
- ✅ CodeQL analysis (Rust and Python)
- ✅ Dependency review on PRs
- ✅ Secret scanning (gitleaks)
- ✅ Automatic security advisories

**Scans:**
1. **cargo-audit** - Check Rust dependencies for known vulnerabilities
2. **cargo-deny** - License compliance and security policy enforcement
3. **pip-audit** - Python package vulnerability scanning
4. **CodeQL** - Static analysis for Rust and Python code
5. **Dependency Review** - PR-based dependency change analysis
6. **Secret Scanning** - Detect exposed secrets in commits

**Usage:**
```bash
# Runs automatically on push/PR
# Daily scheduled scans at 00:00 UTC
# Creates security issues if vulnerabilities found
```

### 6. Code Quality / Lint (`lint.yml`)
**Triggers:** Push, PR, Manual
**Purpose:** Enforce code quality standards

**Checks:**

#### Rust:
- `cargo fmt --check` - Code formatting
- `cargo clippy` - Linting (zero warnings policy)

#### Python:
- `black --check` - Code formatting
- `ruff check` - Linting
- `mypy` - Type checking

#### Additional:
- Commit message format (Conventional Commits)
- Spell checking (codespell)
- File size limits (<500KB)
- TODO/FIXME detection in main branch
- Code coverage reporting

**Usage:**
```bash
# Fix all linting issues locally:
cargo fmt
cargo clippy --fix --allow-dirty
black python/ tests/ benchmarks/
ruff check --fix python/ tests/ benchmarks/

# Run checks before pushing:
cargo fmt --check
cargo clippy -- -D warnings
black --check python/ tests/ benchmarks/
ruff check python/ tests/ benchmarks/
```

## Workflow Best Practices

All workflows implement:

### 1. Concurrency Control
```yaml
concurrency:
  group: workflow-${{ github.ref }}
  cancel-in-progress: true
```
Prevents redundant runs, saves CI minutes.

### 2. Minimal Permissions
```yaml
permissions:
  contents: read
  pull-requests: write
```
Follows principle of least privilege.

### 3. Caching
- Rust dependencies (`Swatinem/rust-cache@v2`)
- Python dependencies (`cache: 'pip'`)
- npm packages (`cache: 'npm'`)

### 4. Timeouts
All jobs have explicit timeouts to prevent hanging builds.

### 5. Job Summaries
Rich markdown summaries in `$GITHUB_STEP_SUMMARY` for all workflows.

### 6. Artifact Upload
Important results saved as artifacts with appropriate retention periods.

### 7. Fail-Fast: False
Matrix builds continue even if one combination fails.

### 8. Error Handling
Continue-on-error for non-critical checks with appropriate warnings.

## Setting Up Workflows

### Required Secrets

Add these in repository Settings → Secrets and variables → Actions:

1. **PYPI_API_TOKEN** (Required for releases)
   - Create at https://pypi.org/manage/account/token/
   - Scope: Project (jsonatapy)

   - Add repository and copy token

### Repository Settings

1. **GitHub Pages**
   - Settings → Pages
   - Source: GitHub Actions

2. **Branch Protection**
   - Settings → Branches
   - Require status checks:
     - All Tests Passed
     - Rust Formatting
     - Rust Clippy
     - Python Formatting
     - Python Linting

3. **Security**
   - Settings → Security → Code security and analysis
   - Enable Dependabot alerts
   - Enable Secret scanning

## Workflow Status Badges

Add to README.md:

```markdown
[![Tests](https://github.com/yourusername/jsonatapy/workflows/Test%20Suite/badge.svg)](https://github.com/yourusername/jsonatapy/actions/workflows/test.yml)
[![Security](https://github.com/yourusername/jsonatapy/workflows/Security%20Scanning/badge.svg)](https://github.com/yourusername/jsonatapy/actions/workflows/security.yml)
[![Code Quality](https://github.com/yourusername/jsonatapy/workflows/Code%20Quality/badge.svg)](https://github.com/yourusername/jsonatapy/actions/workflows/lint.yml)
```

## Monitoring and Maintenance

### Weekly Tasks
- Review security scan results (check email notifications)
- Check benchmark trends on gh-pages
- Review open security advisories

### Monthly Tasks
- Update GitHub Actions versions
- Review and update dependencies
- Check code coverage trends

### Release Checklist
1. All tests passing
2. No security vulnerabilities
3. Code coverage maintained
4. Benchmark regressions addressed
5. Documentation updated
6. CHANGELOG.md updated
7. Version bumped appropriately

## Troubleshooting

### Workflow Not Running
- Check branch protection rules
- Verify workflow file syntax (`yamllint`)
- Check repository permissions

### Cache Issues
```bash
# Clear cache from Actions tab:
# Settings → Actions → Caches → Delete cache
```

### Permission Errors
- Verify `GITHUB_TOKEN` permissions in workflow
- Check repository Settings → Actions → General → Workflow permissions

### Failed PyPI Upload
- Verify `PYPI_API_TOKEN` is set correctly
- Check token hasn't expired
- Ensure version doesn't already exist on PyPI

## Local Testing

### Test Workflow Syntax
```bash
# Install act (Docker-based local runner)
brew install act  # macOS
# or download from https://github.com/nektos/act

# List workflows
act -l

# Run workflow locally
act push -W .github/workflows/test.yml
```

### Validate Workflow Files
```bash
# Install yamllint
pip install yamllint

# Lint workflows
yamllint .github/workflows/
```

## Performance Optimization

Workflows are optimized for:
- Fast feedback (parallel jobs)
- Minimal CI minutes (caching, concurrency control)
- Cost-effective matrix builds (strategic platform selection)
- Quick iteration (separate fast/slow job groups)

## Contributing

When adding new workflows:

1. Follow existing patterns
2. Add comprehensive documentation
3. Include timeout limits
4. Implement caching where applicable
5. Use minimal required permissions
6. Add job summaries
7. Test locally before committing
8. Update this README

## Support

For workflow issues:
1. Check Actions tab for detailed logs
2. Review job summaries
3. Download artifacts for debugging
4. Check GitHub Actions documentation
5. Open an issue with workflow logs

---

**Last Updated:** 2026-02-07
**Maintained by:** jsonatapy contributors
