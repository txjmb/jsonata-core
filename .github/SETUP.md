# GitHub Actions Setup Guide

Quick setup guide for enabling all workflows in the jsonatapy repository.

## Prerequisites

- [ ] Repository admin access
- [ ] PyPI account with API token
- [ ] GitHub account with repository access

## Step 1: Repository Settings

### Enable GitHub Actions
1. Go to repository **Settings**
2. Navigate to **Actions** → **General**
3. Under **Actions permissions**, select: **Allow all actions and reusable workflows**
4. Under **Workflow permissions**, select: **Read and write permissions**
5. Check: **Allow GitHub Actions to create and approve pull requests**
6. Click **Save**

### Enable GitHub Pages
1. Go to **Settings** → **Pages**
2. Under **Source**, select: **GitHub Actions**
3. Click **Save**
4. Note: First deployment will create the gh-pages branch automatically

### Enable Security Features
1. Go to **Settings** → **Security** → **Code security and analysis**
2. Enable the following:
   - ✅ Dependency graph
   - ✅ Dependabot alerts
   - ✅ Dependabot security updates
   - ✅ Secret scanning
   - ✅ Push protection

## Step 2: Add Secrets

### Required Secrets

1. Go to **Settings** → **Secrets and variables** → **Actions**
2. Click **New repository secret**

#### PYPI_API_TOKEN (Required for releases)
```
Name: PYPI_API_TOKEN
Value: pypi-<your-token-here>
```

**How to get it:**
1. Go to https://pypi.org/manage/account/token/
2. Click **Add API token**
3. Token name: `jsonatapy-github-actions`
4. Scope: **Entire account** or **Project: jsonatapy** (if project exists)
5. Click **Add token**
6. Copy the token (starts with `pypi-`)
7. Paste into GitHub secrets

```
Value: <your-codecov-token>
```

**How to get it:**
1. Go to https://codecov.io/
2. Sign in with GitHub
3. Add repository `jsonatapy`
4. Copy the upload token
5. Paste into GitHub secrets

## Step 3: Branch Protection Rules

1. Go to **Settings** → **Branches**
2. Click **Add rule** or edit existing rule for `main`/`master`

### Recommended Settings

```
Branch name pattern: main  (or master)
```

**Protect matching branches:**
- ✅ Require a pull request before merging
  - ✅ Require approvals: 1
  - ✅ Dismiss stale pull request approvals when new commits are pushed

- ✅ Require status checks to pass before merging
  - ✅ Require branches to be up to date before merging

  **Required status checks:**
  - `All Tests Passed` (from test.yml)
  - `Rust Formatting` (from lint.yml)
  - `Rust Clippy` (from lint.yml)
  - `Python Formatting (Black)` (from lint.yml)
  - `Python Linting (Ruff)` (from lint.yml)

- ✅ Require conversation resolution before merging
- ✅ Do not allow bypassing the above settings

## Step 4: Update Repository URLs

Several workflow files contain placeholder URLs that need updating:

### Files to Update

1. **`.github/workflows/docs.yml`**
   ```yaml
   # Line ~10
   repo_url: https://github.com/txjmb/jsonata-core

   # Line ~95
   Homepage = "https://github.com/txjmb/jsonata-core"

   # Line ~206
   # View at: https://txjmb.github.io/jsonata-core/
   ```

2. **`pyproject.toml`**
   ```toml
   [project.urls]
   Homepage = "https://github.com/txjmb/jsonata-core"
   Repository = "https://github.com/txjmb/jsonata-core"
   # etc.
   ```

3. **`Cargo.toml`**
   ```toml
   repository = "https://github.com/txjmb/jsonata-core"
   homepage = "https://github.com/txjmb/jsonata-core"
   ```

**Quick find and replace:**
```bash
# Replace yourusername with your actual GitHub username
find .github pyproject.toml Cargo.toml -type f -exec sed -i 's/yourusername/YOUR_GITHUB_USERNAME/g' {} +

# Replace author information
sed -i 's/Your Name/YOUR_NAME/g' pyproject.toml Cargo.toml
sed -i 's/your.email@example.com/YOUR_EMAIL/g' pyproject.toml Cargo.toml
```

## Step 5: Initial Workflow Run

### Test the Setup

1. **Run Test Workflow**
   - Go to **Actions** tab
   - Select **Test Suite**
   - Click **Run workflow**
   - Verify all jobs pass

2. **Run Lint Workflow**
   - Select **Code Quality**
   - Click **Run workflow**
   - Fix any formatting issues:
     ```bash
     cargo fmt
     black python/ tests/ benchmarks/
     ruff check --fix python/ tests/ benchmarks/
     ```

3. **Run Security Workflow**
   - Select **Security Scanning**
   - Click **Run workflow**
   - Review any security findings

## Step 6: Verify Everything Works

### Checklist

- [ ] Test workflow completes successfully
- [ ] Lint checks pass
- [ ] Security scans complete
- [ ] GitHub Pages is accessible (after first docs deployment)
- [ ] PR comments work (create a test PR)
- [ ] Benchmark workflow runs (push to main or create PR)

### Test with a Pull Request

1. Create a new branch:
   ```bash
   git checkout -b test-workflows
   ```

2. Make a small change (e.g., update README)
   ```bash
   echo "Testing workflows" >> README.md
   git add README.md
   git commit -m "test: Verify workflow automation"
   git push origin test-workflows
   ```

3. Create PR from GitHub UI

4. Verify the following:
   - [ ] Test workflow runs automatically
   - [ ] Lint checks run
   - [ ] Security scans run
   - [ ] Benchmark results posted as comment
   - [ ] All required checks appear in PR

5. Close/delete the test PR after verification

## Step 7: First Release (Optional)

When ready to publish your first release:

1. **Test with dry run:**
   - Go to **Actions** → **Release**
   - Click **Run workflow**
   - Version: `2.0.5.0` (or appropriate version)
   - Dry run: ✅ **true**
   - Click **Run workflow**
   - Verify builds complete successfully

2. **Actual release:**
   - Run workflow again with dry_run: **false**
   - Monitor the workflow
   - Verify package appears on PyPI
   - Check GitHub release is created

## Troubleshooting

### Workflows Not Running
```bash
# Check workflow file syntax
yamllint .github/workflows/

# Verify Actions are enabled in Settings
```

### Permission Errors
- Ensure workflow permissions are set to "Read and write" in Settings → Actions
- Check that secrets are properly set

### PyPI Upload Fails
- Verify PYPI_API_TOKEN is correctly set
- Check token hasn't expired
- Ensure version doesn't already exist on PyPI
- For first publish, package must be created on PyPI first

### Documentation Doesn't Deploy
- Verify GitHub Pages source is set to "GitHub Actions"
- Check docs workflow completed successfully
- Allow 5-10 minutes for first deployment

### Benchmark PR Comments Don't Appear
- Ensure "Allow GitHub Actions to create and approve pull requests" is enabled
- Check workflow permissions include `pull-requests: write`

## Maintenance

### Weekly
- [ ] Review security scan results
- [ ] Check for workflow failures
- [ ] Monitor benchmark trends

### Monthly
- [ ] Update action versions in workflows
- [ ] Review and update dependencies
- [ ] Check code coverage trends

### Before Each Release
- [ ] All tests passing
- [ ] No security vulnerabilities
- [ ] Benchmarks acceptable
- [ ] Documentation updated

## Support

If you encounter issues:

1. Check workflow logs in Actions tab
2. Review job summaries for error details
3. Consult workflow README.md
4. Search GitHub Actions documentation
5. Open an issue with relevant logs

## Advanced Configuration

### Custom Benchmark Schedule
Edit `.github/workflows/benchmark.yml`:
```yaml
schedule:
  # Change from weekly to daily
  - cron: '0 0 * * *'  # Daily at midnight UTC
```

### Adjust Security Scan Frequency
Edit `.github/workflows/security.yml`:
```yaml
schedule:
  # Change from daily to twice daily
  - cron: '0 0,12 * * *'  # At midnight and noon UTC
```

### Enable/Disable Specific Checks

To disable a workflow temporarily:
1. Edit the workflow file
2. Comment out trigger events:
   ```yaml
   on:
     # push:  # Disabled
     #   branches: [main]
     workflow_dispatch:  # Keep manual trigger
   ```

## Next Steps

After setup is complete:

1. Update README.md with status badges
2. Configure branch protection rules
3. Enable required status checks
4. Create first release
5. Share documentation URL with team

---

**Setup Complete!** 🎉

Your repository now has:
- ✅ Automated testing
- ✅ Code quality enforcement
- ✅ Security scanning
- ✅ Performance benchmarking
- ✅ Automated releases
- ✅ Documentation deployment

All workflows will run automatically on push/PR, with scheduled scans for security and performance monitoring.
