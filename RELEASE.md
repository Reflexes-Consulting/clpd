# Release Guide

This document explains how to create releases for the `clpd` project using Git tags.

## Overview

The project uses **semantic versioning** (semver) with tags to trigger automated releases via GitHub Actions. When you push a tag matching the pattern `v*.*.*` (e.g., `v1.0.0`), the workflow will:

1. Run all tests
2. Build the project with the full release profile
3. Create a GitHub Release with the built executable attached

## Semantic Versioning Format

Tags should follow the format: `v<MAJOR>.<MINOR>.<PATCH>`

- **MAJOR**: Increment for breaking/incompatible changes
- **MINOR**: Increment for new features (backward compatible)
- **PATCH**: Increment for bug fixes (backward compatible)

### Examples

- `v1.0.0` - Initial release
- `v1.1.0` - New feature added
- `v1.1.1` - Bug fix
- `v2.0.0` - Breaking changes

## Creating a Release

### Step 1: Update Version in Cargo.toml

Before creating a release tag, update the version in `Cargo.toml`:

```toml
[package]
name = "clpd"
version = "1.0.0"  # Update this line
edition = "2024"
```

Commit this change:

```bash
git add Cargo.toml
git commit -m "Bump version to 1.0.0"
```

### Step 2: Create and Push the Tag

#### Option A: Create Tag Locally and Push

```bash
# Create an annotated tag (recommended)
git tag -a v1.0.0 -m "Release version 1.0.0"

# Push the tag to GitHub
git push origin v1.0.0
```

#### Option B: Create Tag and Push in One Command

```bash
git tag -a v1.0.0 -m "Release version 1.0.0" && git push origin v1.0.0
```

#### Option C: Create Tag via GitHub UI

1. Go to your repository on GitHub
2. Click on "Releases" in the right sidebar
3. Click "Draft a new release"
4. Click "Choose a tag"
5. Type your new tag (e.g., `v1.0.0`) and click "Create new tag"
6. Fill in release details (optional, as the workflow will create the release)
7. Click "Publish release"

### Step 3: Verify the Workflow

1. Go to the **Actions** tab in your GitHub repository
2. You should see a new workflow run triggered by your tag
3. The workflow will:
   - Run the test job
   - Build the project (if tests pass)
   - Create a release (if build succeeds)
4. Once complete, check the **Releases** section to see your new release with the attached `clpd.exe`

## Release Workflow Details

### Workflow Jobs

1. **Test Job** (`test`)

   - Runs all Cargo tests
   - Must pass before building

2. **Build Job** (`build`)

   - Only runs if tests pass
   - Builds with the `full` release profile
   - Uploads the executable as an artifact

3. **Release Job** (`release`)
   - Only runs for tag pushes (not regular commits)
   - Downloads the built artifact
   - Creates a GitHub Release
   - Attaches the `clpd.exe` to the release

### Automatic Release Notes

The workflow automatically generates release notes with:

- Release title: "Release X.Y.Z"
- Basic installation instructions
- Link to commit history

You can customize these by editing `.github/workflows/build.yml`.

## Managing Releases

### Deleting a Tag (If Needed)

If you need to delete a tag:

```bash
# Delete local tag
git tag -d v1.0.0

# Delete remote tag
git push origin :refs/tags/v1.0.0
```

Note: This won't delete the GitHub Release if it was already created.

### Deleting a Release

To delete a GitHub Release:

1. Go to the Releases page
2. Click on the release you want to delete
3. Click "Delete" button
4. Confirm deletion

### Creating a Pre-release

For beta or release candidate versions, use tags like:

- `v1.0.0-beta.1`
- `v1.0.0-rc.1`

You may want to modify the workflow to mark these as pre-releases automatically.

## Best Practices

1. **Always update Cargo.toml version** before tagging
2. **Test locally** before creating a release tag
3. **Use annotated tags** (`git tag -a`) rather than lightweight tags
4. **Write meaningful tag messages** describing what's in the release
5. **Follow semantic versioning** consistently
6. **Review the workflow run** to ensure everything succeeded
7. **Test the released executable** by downloading it from GitHub

## Troubleshooting

### Workflow Doesn't Trigger

- Ensure your tag matches the pattern `v*.*.*`
- Check that you pushed the tag: `git push origin --tags`
- Verify the workflow file is in the correct location

### Tests Fail

- The release won't be created if tests fail
- Check the workflow logs to see which tests failed
- Fix the issues and create a new tag

### Build Fails

- Check that your build configuration is correct
- Ensure all dependencies are available
- Review the workflow logs for specific errors

### Release Not Created

- Verify the `GITHUB_TOKEN` has sufficient permissions
- Check that the workflow reached the release job
- Review the Actions logs for error messages

## Example Release Process

Here's a complete example of releasing version 1.2.3:

```bash
# 1. Make sure you're on the main branch and up to date
git checkout main
git pull origin main

# 2. Update version in Cargo.toml
# Edit Cargo.toml and change version to "1.2.3"

# 3. Commit the version change
git add Cargo.toml
git commit -m "Bump version to 1.2.3"

# 4. Push the commit
git push origin main

# 5. Create and push the tag
git tag -a v1.2.3 -m "Release version 1.2.3 - Add clipboard encryption feature"
git push origin v1.2.3

# 6. Watch the workflow on GitHub Actions
# 7. Verify the release appears in the Releases section
```

## Additional Resources

- [Git Tagging Documentation](https://git-scm.com/book/en/v2/Git-Basics-Tagging)
- [Semantic Versioning](https://semver.org/)
- [GitHub Releases Documentation](https://docs.github.com/en/repositories/releasing-projects-on-github)
