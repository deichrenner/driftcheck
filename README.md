# driftcheck

A fast, reliable pre-push hook that detects documentation drift using LLMs.

When you change code, driftcheck automatically finds related documentation and checks if your changes introduce any
inconsistencies—before you push.

## Features

- **Pre-push hook integration** — Runs automatically before `git push`
- **Smart doc discovery** — Uses LLM to generate targeted ripgrep queries
- **Parallel search** — Finds relevant docs quickly across your codebase
- **Interactive TUI** — Review issues, apply fixes with live progress indicators
- **Git-aware analysis** — Checks recent commits to avoid flagging already-fixed issues
- **Conservative by default** — Only flags clear, factual errors to minimize false positives
- **CI-friendly** — Falls back to text output when no TTY is available
- **Fast** — Single binary, caches LLM queries, shows progress during analysis
- **Flexible LLM backend** — Works with any OpenAI-compatible API (OpenAI, Anthropic via litellm, Ollama, etc.)

## Installation

### Homebrew (macOS/linux)

```bash
brew tap deichrenner/tap
brew install driftcheck
```

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/deichrenner/driftcheck/releases):

```bash
# Linux (x86_64)
curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-linux-x86_64 -o driftcheck
chmod +x driftcheck
sudo mv driftcheck /usr/local/bin/

# Linux (ARM64)
curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-linux-aarch64 -o driftcheck
chmod +x driftcheck
sudo mv driftcheck /usr/local/bin/

# macOS (Apple Silicon)
curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-macos-aarch64 -o driftcheck
chmod +x driftcheck
sudo mv driftcheck /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-macos-x86_64 -o driftcheck
chmod +x driftcheck
sudo mv driftcheck /usr/local/bin/

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-windows-x86_64.exe -OutFile driftcheck.exe
Move-Item driftcheck.exe C:\Windows\System32\
```

### From Source

```bash
# Requires Rust toolchain
cargo install --path .

# Or build manually
cargo build --release
cp target/release/driftcheck ~/.local/bin/
```

### Prerequisites

- [ripgrep](https://github.com/BurntSushi/ripgrep#installation) (`rg`) must be installed
- An OpenAI-compatible LLM API endpoint

## Quick Start

```bash
# Initialize in your repository
cd your-repo
driftcheck init

# Set your API key (or use .env file - see below)
export DRIFTCHECK_API_KEY=your-api-key

# Make some changes, then push—driftcheck runs automatically
git push
```

## Pre-commit Integration

If you use [pre-commit](https://pre-commit.com/), you can add driftcheck to your `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/deichrenner/driftcheck
    rev: v0.1.5
    hooks:
      - id: driftcheck
```

Then install the pre-push hook (required since driftcheck runs on push, not commit):

```bash
pre-commit install --hook-type pre-push
```

This requires driftcheck to be installed on your system (via Homebrew, binary download, or cargo). The hook uses `--no-tui` mode for compatibility with pre-commit's output handling.

You'll still need to:

1. Create a `.driftcheck.toml` config file (or run `driftcheck init` once to generate one)
2. Set your `DRIFTCHECK_API_KEY` environment variable

## How It Works

```
┌────────────────────────────────────────────────────────────┐
│                           PRE-PUSH HOOK                    │
├────────────────────────────────────────────────────────────┤
│                                                            │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ 1. Get Diff  │───▶│ 2. Generate  │───▶│ 3. Search    │  │
│  │              │    │ RG Queries   │    │ Docs (rg)    │  │
│  │ git diff     │    │ (LLM call)   │    │              │  │
│  │ @{u}..HEAD   │    │              │    │ Parallel     │  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
│         │                                       │          │
│         ▼                                       ▼          │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ Get Recent   │    │ 6. Output    │◀───│ 4. Analyze   │  │
│  │ Git Log      │───▶│              │    │ Consistency  │  │
│  │ (context)    │    │ TUI / Error  │    │ (LLM call)   │  │
│  └──────────────┘    └──────────────┘    └──────────────┘  │
│                                                            │
└────────────────────────────────────────────────────────────┘
```

### Git Flow Support

When pushing a new feature branch for the first time (before setting an upstream), driftcheck automatically falls back to comparing against the default branch:

1. **Upstream tracking branch** (`@{u}`) — Used if available
2. **Config `fallback_base`** — If set in `.driftcheck.toml`
3. **Auto-detected default branch** — Checks `origin/HEAD`, then `origin/main`, then `origin/master`

This ensures new branches are still checked for documentation drift without requiring manual configuration.

### Analysis Approach

driftcheck is intentionally **conservative** to minimize false positives:

- **Only flags factual errors** — Documentation that explicitly contradicts the code
- **Checks git history** — Reviews recent commits to avoid flagging issues you've already fixed
- **Ignores stylistic issues** — Won't complain about missing docs or suggestions for improvement

## Commands

```bash
driftcheck init              # Initialize in current repo (creates config + hook)
driftcheck check             # Run analysis manually
driftcheck check --range REF # Check specific commit range
driftcheck check --no-tui    # Force non-interactive output

driftcheck config            # Show current configuration
driftcheck config --edit     # Open config in $EDITOR
driftcheck config --path     # Show config file path

driftcheck enable            # Enable driftcheck
driftcheck disable           # Disable without uninstalling

driftcheck cache clear       # Clear cached queries
driftcheck cache stats       # Show cache statistics

driftcheck install-hook      # Reinstall the pre-push hook
```

## Configuration

Configuration is stored in `.driftcheck.toml` (or `driftcheck.toml`) in your repo root:

```toml
[general]
enabled = true
allow_push_on_error = false  # If true, push proceeds even on LLM errors
# fallback_base = "origin/main"  # Used when no upstream (e.g., new branch push)

[docs]
paths = [
    "README.md",
    "docs/**/*.md",
]
ignore = [
    "docs/archive/**",
    "CHANGELOG.md",
]
max_context_tokens = 8000  # Limit doc context sent to LLM

[llm]
base_url = "https://api.openai.com/v1"  # Or your litellm proxy
model = "gpt-4o"
timeout = 30
max_retries = 2

[tui]
theme = "default"  # "default", "minimal", or "colorful"
auto_apply = false

[cache]
enabled = true
dir = ".git/driftcheck_cache"
ttl = 3600  # Cache TTL in seconds

[prompts]
# You can customize the analysis prompt to be more or less strict
# analysis = "Your custom prompt here..."
```

## Secrets & API Keys

driftcheck supports multiple ways to provide your API key:

### 1. Environment Variable (simplest)

```bash
export DRIFTCHECK_API_KEY=sk-...
```

### 2. `.env` File (recommended for local dev)

Create a `.env` file in your repo root (add to `.gitignore`!):

```bash
# .env
DRIFTCHECK_API_KEY=sk-...
```

driftcheck automatically loads `.env` files from:

1. Git repository root
2. Current working directory

### 3. File Path (recommended for CI)

Point to a file containing the API key:

```bash
export DRIFTCHECK_API_KEY_FILE=/path/to/secret/api-key
```

This is useful for CI systems that write secrets to temporary files.

### Environment Variables Reference

| Variable                  | Description                     |
|---------------------------|---------------------------------|
| `DRIFTCHECK_API_KEY`      | LLM API key                     |
| `DRIFTCHECK_API_KEY_FILE` | Path to file containing API key |
| `DRIFTCHECK_CONFIG`       | Custom config file path         |
| `DRIFTCHECK_DISABLED=1`   | Disable without editing config  |
| `DRIFTCHECK_DEBUG=1`      | Enable verbose logging          |

## CI Integration

### GitHub Actions

Add to your workflow:

```yaml
name: Documentation Check

on:
  pull_request:
    branches: [ main ]

jobs:
  driftcheck:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Need full history for diff

      - name: Install ripgrep
        run: sudo apt-get install -y ripgrep

      - name: Install driftcheck
        run: |
          curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-linux-x86_64 -o driftcheck
          chmod +x driftcheck
          sudo mv driftcheck /usr/local/bin/

      - name: Check documentation
        env:
          DRIFTCHECK_API_KEY: ${{ secrets.DRIFTCHECK_API_KEY }}
        run: driftcheck check --range origin/${{ github.base_ref }}..HEAD --no-tui
```

### GitLab CI

```yaml
driftcheck:
  image: debian:bookworm-slim
  before_script:
    - apt-get update && apt-get install -y curl ripgrep
    - curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-linux-x86_64 -o /usr/local/bin/driftcheck
    - chmod +x /usr/local/bin/driftcheck
  script:
    - driftcheck check --range origin/$CI_MERGE_REQUEST_TARGET_BRANCH_NAME..HEAD --no-tui
  variables:
    DRIFTCHECK_API_KEY: $DRIFTCHECK_API_KEY
  rules:
    - if: $CI_PIPELINE_SOURCE == "merge_request_event"
```

### CircleCI

```yaml
version: 2.1

jobs:
  driftcheck:
    docker:
      - image: cimg/base:stable
    steps:
      - checkout
      - run:
          name: Install dependencies
          command: |
            sudo apt-get update && sudo apt-get install -y ripgrep
            curl -L https://github.com/deichrenner/driftcheck/releases/latest/download/driftcheck-linux-x86_64 -o driftcheck
            chmod +x driftcheck
            sudo mv driftcheck /usr/local/bin/
      - run:
          name: Check documentation
          command: driftcheck check --range origin/main..HEAD --no-tui
          environment:
            DRIFTCHECK_API_KEY: ${DRIFTCHECK_API_KEY}

workflows:
  check:
    jobs:
      - driftcheck
```

## TUI Keybindings

When issues are detected in a TTY, driftcheck launches an interactive TUI:

| Key         | Action                                            |
|-------------|---------------------------------------------------|
| `a`         | Apply fix (generates fix via LLM, writes to file) |
| `s`         | Skip this issue                                   |
| `j` / `↓`   | Next issue                                        |
| `k` / `↑`   | Previous issue                                    |
| `Enter`     | Confirm all and continue push                     |
| `q` / `Esc` | Abort push                                        |
| `?`         | Show help                                         |

### Apply Fix Workflow

When you press `a` to apply a fix:

1. A spinner appears showing the fix is being generated
2. The LLM generates the complete fixed documentation
3. The file is updated in place
4. The issue is marked as "Applied" with a checkmark
5. You automatically move to the next pending issue

After exiting the TUI, review all changes with `git diff` before committing.

### Issue States

| Symbol | State    | Description                            |
|--------|----------|----------------------------------------|
| `○`    | Pending  | Not yet addressed                      |
| `⠋`    | Applying | Fix being generated (animated spinner) |
| `✓`    | Applied  | Fix has been written to file           |
| `⊘`    | Skipped  | Manually skipped                       |
| `✗`    | Error    | Fix generation failed                  |

## Using with Different LLM Providers

### OpenAI (default)

```bash
export DRIFTCHECK_API_KEY=sk-...
```

### Anthropic (via litellm)

```bash
# Start litellm proxy
litellm --model claude-sonnet-4-20250514

# Configure driftcheck
# In .driftcheck.toml:
# [llm]
# base_url = "http://localhost:4000"
# model = "claude-sonnet-4-20250514"
```

### Ollama (local)

```bash
# In .driftcheck.toml:
# [llm]
# base_url = "http://localhost:11434/v1"
# model = "llama2"

# Ollama doesn't need an API key, but set a dummy value
export DRIFTCHECK_API_KEY=ollama
```

### OpenRouter

```bash
# In .driftcheck.toml:
# [llm]
# base_url = "https://openrouter.ai/api/v1"
# model = "anthropic/claude-3.5-sonnet"

export DRIFTCHECK_API_KEY=sk-or-...
```

## Behavior Matrix

| Scenario           | TTY Available | Action                                 |
|--------------------|---------------|----------------------------------------|
| No issues detected | Yes/No        | Push proceeds                          |
| Issues detected    | Yes           | Launch TUI for review                  |
| Issues detected    | No            | Block push, print errors               |
| LLM timeout/error  | Yes/No        | Warn, proceed if `allow_push_on_error` |
| Config missing     | Yes/No        | Block, print setup instructions        |

## Reducing False Positives

driftcheck is designed to be conservative, but if you're still seeing too many false positives:

1. **Clear the cache** after updating: `driftcheck cache clear`
2. **Customize the prompt** in `.driftcheck.toml` to be stricter
3. **Narrow doc paths** to only check the most critical documentation
4. **Use ignore patterns** to exclude generated or less important docs

The default prompt only flags issues where documentation is **factually wrong** due to code changes. It ignores:

- Missing documentation for new features
- Stylistic suggestions
- Vague but technically correct documentation
- Issues in files that were recently modified (assumes you fixed them)

## Bypassing the Hook

If you need to push without running driftcheck:

```bash
git push --no-verify
```

## Development

```bash
# Build
cargo build
cargo build --release

# Run tests
cargo test

# Lint
cargo clippy
cargo fmt
```

## License

MIT
