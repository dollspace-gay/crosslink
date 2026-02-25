//! Build script to track include_str! dependencies.
//! This ensures cargo rebuilds when template files change.

fn main() {
    // Track claude resource files
    println!("cargo:rerun-if-changed=resources/claude/settings.json");
    println!("cargo:rerun-if-changed=resources/claude/hooks/prompt-guard.py");
    println!("cargo:rerun-if-changed=resources/claude/hooks/post-edit-check.py");
    println!("cargo:rerun-if-changed=resources/claude/hooks/session-start.py");
    println!("cargo:rerun-if-changed=resources/claude/hooks/pre-web-check.py");
    println!("cargo:rerun-if-changed=resources/claude/hooks/work-check.py");
    println!("cargo:rerun-if-changed=resources/claude/mcp/safe-fetch-server.py");
    println!("cargo:rerun-if-changed=resources/mcp.json");

    // Track chainlink config and rules files
    println!("cargo:rerun-if-changed=resources/chainlink/hook-config.json");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/global.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/project.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/rust.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/python.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/javascript.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/typescript.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/typescript-react.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/javascript-react.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/go.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/java.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/c.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/cpp.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/csharp.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/ruby.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/php.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/swift.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/kotlin.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/scala.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/zig.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/odin.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/elixir.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/elixir-phoenix.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/web.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/sanitize-patterns.txt");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/tracking-strict.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/tracking-normal.md");
    println!("cargo:rerun-if-changed=resources/chainlink/rules/tracking-relaxed.md");
}
