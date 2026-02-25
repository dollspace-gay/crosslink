#!/usr/bin/env python3
"""
Crosslink Context Provider - Agent-Agnostic AI Context Injection

This script generates context for any AI coding assistant by:
1. Detecting project languages and structure
2. Loading session state and issues
3. Applying relevant coding rules
4. Outputting in a format any LLM understands

Usage:
    # Get full context (session + issues + rules)
    python context-provider.py

    # Get specific context types
    python context-provider.py --session      # Session context only
    python context-provider.py --issues       # Issues context only
    python context-provider.py --rules        # Coding rules only
    python context-provider.py --structure    # Project structure only

    # Output formats
    python context-provider.py --format xml   # XML tags (default)
    python context-provider.py --format md    # Markdown
    python context-provider.py --format json  # JSON

    # Integration modes
    python context-provider.py --prepend "user prompt here"  # Prepend to prompt
    python context-provider.py --env          # Output as env vars
    python context-provider.py --clipboard    # Copy to clipboard

Examples for different agents:
    # Aider
    python context-provider.py > /tmp/context.md && aider --message-file /tmp/context.md

    # Generic wrapper
    CONTEXT=$(python context-provider.py) && echo "$CONTEXT\n\nUser: $1" | llm

    # Cursor (.cursorrules integration)
    python context-provider.py --format md --rules >> .cursorrules
"""

import argparse
import io
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Optional

# Fix Windows encoding issues
if sys.stdout.encoding != 'utf-8':
    sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8', errors='replace')
if sys.stderr.encoding != 'utf-8':
    sys.stderr = io.TextIOWrapper(sys.stderr.buffer, encoding='utf-8', errors='replace')


def find_crosslink_dir() -> Optional[Path]:
    """Find .crosslink directory by walking up from cwd."""
    current = Path.cwd()
    while current != current.parent:
        crosslink_dir = current / ".crosslink"
        if crosslink_dir.exists():
            return crosslink_dir
        current = current.parent
    return None


def run_crosslink(args: list[str]) -> tuple[str, bool]:
    """Run a crosslink command and return output."""
    try:
        result = subprocess.run(
            ["crosslink"] + args,
            capture_output=True,
            text=True,
            timeout=10
        )
        return result.stdout.strip(), result.returncode == 0
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return "", False


def detect_languages() -> list[str]:
    """Detect programming languages in the project."""
    languages = []
    cwd = Path.cwd()

    # Language indicators
    indicators = {
        "rust": ["Cargo.toml", "*.rs"],
        "python": ["pyproject.toml", "setup.py", "requirements.txt", "*.py"],
        "typescript": ["tsconfig.json", "*.ts", "*.tsx"],
        "javascript": ["package.json", "*.js", "*.jsx"],
        "go": ["go.mod", "go.sum", "*.go"],
        "java": ["pom.xml", "build.gradle", "*.java"],
        "c": ["*.c", "*.h", "Makefile"],
        "cpp": ["*.cpp", "*.hpp", "*.cc", "CMakeLists.txt"],
        "csharp": ["*.csproj", "*.cs", "*.sln"],
        "ruby": ["Gemfile", "*.rb"],
        "php": ["composer.json", "*.php"],
        "swift": ["Package.swift", "*.swift"],
        "kotlin": ["*.kt", "build.gradle.kts"],
        "scala": ["build.sbt", "*.scala"],
        "zig": ["build.zig", "*.zig"],
    }

    for lang, patterns in indicators.items():
        for pattern in patterns:
            if pattern.startswith("*"):
                # Glob pattern - check if any files match
                if list(cwd.rglob(pattern))[:1]:  # Just check if any exist
                    languages.append(lang)
                    break
            else:
                # Exact file
                if (cwd / pattern).exists():
                    languages.append(lang)
                    break

    return languages


def get_project_structure(max_depth: int = 3, max_entries: int = 50) -> str:
    """Get project directory structure."""
    cwd = Path.cwd()
    entries = []
    count = 0

    def walk(path: Path, depth: int, prefix: str = ""):
        nonlocal count
        if depth > max_depth or count >= max_entries:
            return

        try:
            items = sorted(path.iterdir(), key=lambda x: (x.is_file(), x.name))
        except PermissionError:
            return

        # Filter out common noise
        skip = {".git", "node_modules", "__pycache__", ".venv", "venv",
                "target", "dist", "build", ".next", ".cache", "coverage"}
        items = [i for i in items if i.name not in skip]

        for i, item in enumerate(items):
            if count >= max_entries:
                entries.append(f"{prefix}... (truncated at {max_entries} entries)")
                return

            is_last = i == len(items) - 1
            connector = "└── " if is_last else "├── "
            entries.append(f"{prefix}{connector}{item.name}{'/' if item.is_dir() else ''}")
            count += 1

            if item.is_dir():
                extension = "    " if is_last else "│   "
                walk(item, depth + 1, prefix + extension)

    walk(cwd, 0)
    return "\n".join(entries)


def get_session_context() -> dict:
    """Get current crosslink session context."""
    context = {
        "active": False,
        "session_id": None,
        "active_issue": None,
        "handoff_notes": None,
        "ready_issues": [],
        "open_issues": [],
    }

    # Get session status
    output, success = run_crosslink(["session", "status"])
    if success and output:
        context["active"] = "Session #" in output
        for line in output.split("\n"):
            if "Session #" in line:
                try:
                    context["session_id"] = int(line.split("#")[1].split()[0])
                except (IndexError, ValueError):
                    context["session_id"] = None  # Parse failed, leave as None
            if "Working on:" in line:
                context["active_issue"] = line.split("Working on:")[1].strip()
            if "Handoff notes:" in line:
                # Get the rest as handoff notes
                idx = output.find("Handoff notes:")
                if idx != -1:
                    context["handoff_notes"] = output[idx + 14:].strip()

    # Get ready issues
    output, success = run_crosslink(["ready"])
    if success and output and "No ready issues" not in output:
        for line in output.split("\n"):
            if line.strip().startswith("#"):
                context["ready_issues"].append(line.strip())

    # Get open issues
    output, success = run_crosslink(["list"])
    if success and output and "No issues found" not in output:
        for line in output.split("\n"):
            if line.strip().startswith("#") or line.strip().startswith("  #"):
                context["open_issues"].append(line.strip())

    return context


def get_coding_rules(languages: list[str]) -> str:
    """Get coding rules for detected languages."""
    rules = []

    # General rules (always included)
    rules.append("""### General Requirements
1. **NO STUBS**: Never write placeholder comments, empty bodies, or incomplete markers as implementation
2. **NO DEAD CODE**: Remove unused code, don't comment it out
3. **FULL FEATURES**: Implement complete features, don't stop partway
4. **ERROR HANDLING**: Proper error handling everywhere, no panics on bad input
5. **SECURITY**: Validate input, use parameterized queries, no command injection
6. **READ BEFORE WRITE**: Always read a file before editing it""")

    # Language-specific rules
    lang_rules = {
        "rust": """### Rust Best Practices
- Use `rustfmt` and `clippy` before committing
- Prefer `?` operator over `.unwrap()` for error handling
- Use `anyhow::Result` for application errors
- Avoid `.clone()` unless necessary - prefer references
- Use `&str` for parameters, `String` for owned data
- Always use parameterized queries with `params![]` for SQL""",

        "python": """### Python Best Practices
- Use type hints for function signatures
- Use `pathlib.Path` over `os.path`
- Use context managers for file/resource handling
- Prefer list comprehensions over map/filter
- Use `logging` module, not print for debugging
- Never use `eval()` or `exec()` with user input""",

        "typescript": """### TypeScript Best Practices
- Enable strict mode in tsconfig.json
- Prefer `interface` over `type` for object shapes
- Use `const` by default, `let` when needed, never `var`
- Use explicit return types on functions
- Handle null/undefined explicitly
- Never use `any` without justification""",

        "javascript": """### JavaScript Best Practices
- Use `const` by default, `let` when needed, never `var`
- Use arrow functions for callbacks
- Use template literals over string concatenation
- Use destructuring for object/array access
- Never use `eval()` or `innerHTML` with user input
- Use `textContent` instead of `innerHTML` when possible""",

        "go": """### Go Best Practices
- Run `go fmt` and `go vet` before committing
- Handle errors explicitly, don't ignore them
- Use meaningful variable names, not single letters
- Prefer table-driven tests
- Use context.Context for cancellation
- Never use `panic()` for error handling""",

        "java": """### Java Best Practices
- Follow standard naming conventions (camelCase, PascalCase)
- Use try-with-resources for AutoCloseable
- Prefer composition over inheritance
- Use Optional for nullable returns
- Use PreparedStatement for SQL queries
- Don't catch Exception, catch specific types""",

        "c": """### C Best Practices
- Always check return values of system calls
- Free allocated memory, avoid leaks
- Use `const` for read-only parameters
- Bounds-check all array access
- Never use `gets()`, use `fgets()`
- Initialize all variables before use""",

        "cpp": """### C++ Best Practices
- Use smart pointers (unique_ptr, shared_ptr)
- Prefer RAII for resource management
- Use `const` and `constexpr` where possible
- Prefer range-based for loops
- Use `std::string_view` for read-only strings
- Never use raw `new`/`delete` in modern code""",
    }

    for lang in languages:
        if lang in lang_rules:
            rules.append(lang_rules[lang])

    return "\n\n".join(rules)


def format_output(context: dict, fmt: str = "xml") -> str:
    """Format the context for output."""

    if fmt == "json":
        return json.dumps(context, indent=2)

    parts = []

    # Session context
    if context.get("session"):
        session = context["session"]
        if fmt == "xml":
            parts.append("<crosslink-session>")
            if session["active"]:
                parts.append(f"Session #{session['session_id']} active")
                if session["active_issue"]:
                    parts.append(f"Working on: {session['active_issue']}")
                if session["handoff_notes"]:
                    parts.append(f"Handoff notes: {session['handoff_notes']}")
            else:
                parts.append("No active session. Use 'crosslink session start' to begin.")
            parts.append("</crosslink-session>")
        else:  # markdown
            parts.append("## Crosslink Session")
            if session["active"]:
                parts.append(f"- **Session:** #{session['session_id']}")
                if session["active_issue"]:
                    parts.append(f"- **Working on:** {session['active_issue']}")
                if session["handoff_notes"]:
                    parts.append(f"- **Handoff notes:** {session['handoff_notes']}")
            else:
                parts.append("No active session.")

    # Issues context
    if context.get("issues"):
        issues = context["issues"]
        if fmt == "xml":
            parts.append("<crosslink-issues>")
            if issues["ready"]:
                parts.append("Ready issues (unblocked):")
                for issue in issues["ready"]:
                    parts.append(f"  {issue}")
            if issues["open"]:
                parts.append("Open issues:")
                for issue in issues["open"]:
                    parts.append(f"  {issue}")
            if not issues["ready"] and not issues["open"]:
                parts.append("No open issues.")
            parts.append("</crosslink-issues>")
        else:
            parts.append("## Issues")
            if issues["ready"]:
                parts.append("### Ready (unblocked)")
                for issue in issues["ready"]:
                    parts.append(f"- {issue}")
            if issues["open"]:
                parts.append("### Open")
                for issue in issues["open"]:
                    parts.append(f"- {issue}")

    # Project structure
    if context.get("structure"):
        if fmt == "xml":
            parts.append("<project-structure>")
            parts.append(f"Languages: {', '.join(context['languages'])}")
            parts.append("```")
            parts.append(context["structure"])
            parts.append("```")
            parts.append("</project-structure>")
        else:
            parts.append("## Project Structure")
            parts.append(f"**Languages:** {', '.join(context['languages'])}")
            parts.append("```")
            parts.append(context["structure"])
            parts.append("```")

    # Coding rules
    if context.get("rules"):
        if fmt == "xml":
            parts.append("<coding-rules>")
            parts.append(context["rules"])
            parts.append("</coding-rules>")
        else:
            parts.append("## Coding Rules")
            parts.append(context["rules"])

    # Workflow reminder
    if context.get("session") or context.get("issues"):
        if fmt == "xml":
            parts.append("<workflow-reminder>")
        else:
            parts.append("## Workflow")
        parts.append("- Use `crosslink session start` at the beginning of work")
        parts.append("- Use `crosslink session work <id>` to mark current focus")
        parts.append("- Add comments: `crosslink comment <id> \"...\"`")
        parts.append("- End with notes: `crosslink session end --notes \"...\"`")
        if fmt == "xml":
            parts.append("</workflow-reminder>")

    return "\n".join(parts)


def main():
    parser = argparse.ArgumentParser(
        description="Generate AI context from crosslink project state",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )

    # Context type flags
    parser.add_argument("--session", action="store_true", help="Include session context")
    parser.add_argument("--issues", action="store_true", help="Include issues context")
    parser.add_argument("--rules", action="store_true", help="Include coding rules")
    parser.add_argument("--structure", action="store_true", help="Include project structure")
    parser.add_argument("--all", action="store_true", help="Include everything (default)")

    # Output format
    parser.add_argument("--format", "-f", choices=["xml", "md", "json"],
                        default="xml", help="Output format")

    # Integration modes
    parser.add_argument("--prepend", metavar="PROMPT", help="Prepend context to a prompt")
    parser.add_argument("--env", action="store_true", help="Output as environment variables")
    parser.add_argument("--clipboard", action="store_true", help="Copy to clipboard")

    args = parser.parse_args()

    # Default to all if no specific flags
    include_all = args.all or not (args.session or args.issues or args.rules or args.structure)

    # Check for crosslink project
    crosslink_dir = find_crosslink_dir()

    # Detect languages
    languages = detect_languages()

    # Build context
    context = {"languages": languages}

    if include_all or args.session or args.issues:
        session_ctx = get_session_context()
        if include_all or args.session:
            context["session"] = {
                "active": session_ctx["active"],
                "session_id": session_ctx["session_id"],
                "active_issue": session_ctx["active_issue"],
                "handoff_notes": session_ctx["handoff_notes"],
            }
        if include_all or args.issues:
            context["issues"] = {
                "ready": session_ctx["ready_issues"],
                "open": session_ctx["open_issues"],
            }

    if include_all or args.structure:
        context["structure"] = get_project_structure()

    if include_all or args.rules:
        context["rules"] = get_coding_rules(languages)

    # Format output
    output = format_output(context, args.format)

    # Handle integration modes
    if args.prepend:
        output = f"{output}\n\n---\n\nUser request: {args.prepend}"

    if args.env:
        # Output as shell-friendly env vars
        print(f'CROSSLINK_LANGUAGES="{",".join(languages)}"')
        if context.get("session"):
            print(f'CROSSLINK_SESSION_ACTIVE={"1" if context["session"]["active"] else "0"}')
            if context["session"]["session_id"]:
                print(f'CROSSLINK_SESSION_ID="{context["session"]["session_id"]}"')
        print(f'CROSSLINK_CONTEXT="{output.replace(chr(10), "\\n").replace(chr(34), "\\\"")}"')
        return

    if args.clipboard:
        try:
            if sys.platform == "darwin":
                subprocess.run(["pbcopy"], input=output.encode(), check=True)
            elif sys.platform == "win32":
                subprocess.run(["clip"], input=output.encode(), check=True)
            else:
                subprocess.run(["xclip", "-selection", "clipboard"],
                             input=output.encode(), check=True)
            print("Context copied to clipboard", file=sys.stderr)
        except (subprocess.CalledProcessError, FileNotFoundError):
            print("Failed to copy to clipboard", file=sys.stderr)
            print(output)
        return

    print(output)


if __name__ == "__main__":
    main()
