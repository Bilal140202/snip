use std::collections::HashMap;

/// The kind of token identified in a command string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// An executable binary (the first token, or the binary after a pipe/&&).
    Binary,
    /// A flag like `--release`, `-v`, `--verbose`.
    Flag,
    /// A value/argument passed to a flag or command.
    Argument,
    /// A pipe operator `|`.
    Pipe,
    /// A redirect like `>`, `>>`, `2>`, `<`.
    Redirect,
    /// A shell operator like `&&`, `||`, `;`.
    Operator,
    /// Anything we can't classify.
    Unknown,
}

/// A single token with its explanation.
#[derive(Debug, Clone)]
pub struct ExplanationPart {
    pub token: String,
    pub explanation: String,
    pub kind: TokenKind,
}

/// A segment of a piped command.
#[derive(Debug, Clone)]
pub struct PipeSegment {
    pub command: String,
    pub position: usize,
}

// ── Knowledge base ────────────────────────────────────────────────────

/// Returns a static knowledge base mapping `"binary subcommand"` to an explanation.
fn knowledge_base() -> HashMap<&'static str, &'static str> {
    let mut kb = HashMap::new();

    // cargo
    kb.insert("cargo", "Rust package manager");
    kb.insert("cargo build", "Compile the project");
    kb.insert("cargo test", "Run all tests");
    kb.insert("cargo run", "Build and execute the binary");
    kb.insert("cargo clippy", "Lint the project for common mistakes");
    kb.insert("cargo check", "Type-check without producing a binary (fast)");
    kb.insert("cargo fmt", "Format code with rustfmt");
    kb.insert("cargo doc", "Generate documentation from doc comments");
    kb.insert("cargo publish", "Publish the crate to crates.io");
    kb.insert("cargo add", "Add a dependency to Cargo.toml");
    kb.insert("cargo remove", "Remove a dependency from Cargo.toml");
    kb.insert("cargo update", "Update dependencies in Cargo.lock");
    kb.insert("cargo clean", "Remove build artifacts (target/ directory)");

    // npm / yarn / pnpm
    kb.insert("npm", "Node.js package manager");
    kb.insert("npm install", "Install all dependencies from package.json");
    kb.insert("npm run", "Run a script defined in package.json");
    kb.insert("npm run build", "Run the build script");
    kb.insert("npm run test", "Run the test script");
    kb.insert("npm run dev", "Start the development server");
    kb.insert("npm run start", "Start the production server");
    kb.insert("npm run lint", "Run the linter");
    kb.insert("npm init", "Create a new package.json");
    kb.insert("npm publish", "Publish the package to npm");

    kb.insert("yarn", "Fast, reliable Node.js package manager");
    kb.insert("yarn install", "Install all dependencies");
    kb.insert("yarn build", "Build the project");
    kb.insert("yarn test", "Run tests");
    kb.insert("yarn dev", "Start the development server");
    kb.insert("yarn start", "Start the production server");
    kb.insert("yarn lint", "Run the linter");
    kb.insert("yarn add", "Add a dependency");

    kb.insert("pnpm", "Fast, disk-space efficient Node.js package manager");
    kb.insert("pnpm install", "Install all dependencies");
    kb.insert("pnpm build", "Build the project");
    kb.insert("pnpm test", "Run tests");
    kb.insert("pnpm dev", "Start the development server");
    kb.insert("pnpm start", "Start the production server");
    kb.insert("pnpm lint", "Run the linter");
    kb.insert("pnpm add", "Add a dependency");

    // git
    kb.insert("git", "Distributed version control system");
    kb.insert("git add", "Stage changes for the next commit");
    kb.insert("git commit", "Record staged changes with a message");
    kb.insert("git push", "Upload local commits to a remote repository");
    kb.insert("git pull", "Fetch and merge changes from a remote repository");
    kb.insert("git checkout", "Switch branches or restore working tree files");
    kb.insert("git switch", "Switch to a different branch");
    kb.insert("git branch", "List, create, or delete branches");
    kb.insert("git merge", "Join two or more development histories together");
    kb.insert("git rebase", "Reapply commits on top of another base tip");
    kb.insert("git stash", "Stash the changes in a dirty working directory");
    kb.insert("git stash pop", "Apply and remove the latest stash entry");
    kb.insert("git stash list", "List all stash entries");
    kb.insert("git log", "Show commit logs");
    kb.insert("git diff", "Show changes between commits, commit and working tree, etc.");
    kb.insert("git status", "Show the working tree status");
    kb.insert("git fetch", "Download objects and refs from another repository");
    kb.insert("git reset", "Reset current HEAD to the specified state");
    kb.insert("git cherry-pick", "Apply changes introduced by some existing commits");
    kb.insert("git tag", "Create, list, delete, or verify a tag object");
    kb.insert("git remote", "Manage set of tracked repositories");

    // docker / docker-compose
    kb.insert("docker", "Container runtime for building and running containers");
    kb.insert("docker build", "Build an image from a Dockerfile");
    kb.insert("docker run", "Run a command in a new container");
    kb.insert("docker ps", "List running containers");
    kb.insert("docker ps -a", "List all containers (including stopped)");
    kb.insert("docker logs", "Fetch the logs of a container");
    kb.insert("docker exec", "Run a command inside a running container");
    kb.insert("docker stop", "Stop one or more running containers");
    kb.insert("docker rm", "Remove one or more containers");
    kb.insert("docker rmi", "Remove one or more images");
    kb.insert("docker pull", "Pull an image from a registry");
    kb.insert("docker push", "Push an image to a registry");
    kb.insert("docker-compose", "Define and run multi-container applications");
    kb.insert("docker compose", "Define and run multi-container applications (v2 CLI plugin)");
    kb.insert("docker-compose up", "Create and start all services in the compose file");
    kb.insert("docker-compose down", "Stop and remove containers, networks, and images");
    kb.insert("docker-compose build", "Build or rebuild services");
    kb.insert("docker-compose ps", "List running containers for the compose project");
    kb.insert("docker-compose logs", "View output from containers");
    kb.insert("docker-compose exec", "Execute a command in a running service container");
    kb.insert("docker compose up", "Create and start all services (v2 CLI plugin)");
    kb.insert("docker compose down", "Stop and remove containers (v2 CLI plugin)");
    kb.insert("docker compose build", "Build or rebuild services (v2 CLI plugin)");
    kb.insert("docker compose ps", "List containers for the compose project (v2 CLI plugin)");
    kb.insert("docker compose logs", "View output from containers (v2 CLI plugin)");
    kb.insert("docker compose exec", "Execute a command in a running container (v2 CLI plugin)");

    // kubectl
    kb.insert("kubectl", "Command-line tool for controlling Kubernetes clusters");
    kb.insert("kubectl apply", "Apply a configuration to a resource by file name or stdin");
    kb.insert("kubectl delete", "Delete resources by file names, stdin, resources and names, or by resources and label selector");
    kb.insert("kubectl get", "Display one or many resources");
    kb.insert("kubectl describe", "Show details of a specific resource or group of resources");
    kb.insert("kubectl logs", "Print the logs for a container in a pod");
    kb.insert("kubectl rollout", "Manage the rollout of a resource");
    kb.insert("kubectl config", "Modify kubeconfig files");
    kb.insert("kubectl port-forward", "Forward one or more local ports to a pod");

    // search / text processing
    kb.insert("grep", "Search for patterns in text using basic regex");
    kb.insert("rg", "Ripgrep: fast recursive search (respects .gitignore)");
    kb.insert("find", "Search for files in a directory hierarchy");
    kb.insert("fd", "A simple, fast, and user-friendly alternative to find");
    kb.insert("sed", "Stream editor for filtering and transforming text");
    kb.insert("awk", "Pattern scanning and text processing language");
    kb.insert("cat", "Concatenate files and print on standard output");
    kb.insert("head", "Output the first part of files");
    kb.insert("tail", "Output the last part of files");
    kb.insert("sort", "Sort lines of text files");
    kb.insert("uniq", "Report or omit repeated lines");
    kb.insert("wc", "Print newline, word, and byte counts for each file");
    kb.insert("cut", "Remove sections from each line of a file");
    kb.insert("tr", "Translate or delete characters");
    kb.insert("xargs", "Build and execute command lines from standard input");
    kb.insert("jq", "Command-line JSON processor");
    kb.insert("curl", "Transfer data to or from a server");
    kb.insert("wget", "Non-interactive network downloader");
    kb.insert("chmod", "Change file mode bits (permissions)");
    kb.insert("chown", "Change file owner and group");
    kb.insert("cp", "Copy files and directories");
    kb.insert("mv", "Move or rename files and directories");
    kb.insert("rm", "Remove files or directories");
    kb.insert("mkdir", "Create directories");
    kb.insert("ls", "List directory contents");
    kb.insert("echo", "Display a line of text");
    kb.insert("printf", "Format and print data");
    kb.insert("cd", "Change the shell working directory");
    kb.insert("pwd", "Print the name of the current working directory");
    kb.insert("env", "Display or set environment variables");
    kb.insert("export", "Set environment variables for child processes");
    kb.insert("source", "Execute commands from a file in the current shell");
    kb.insert("make", "Build automation tool driven by a Makefile");
    kb.insert("cmake", "Cross-platform build system generator");
    kb.insert("python", "Python interpreter");
    kb.insert("python3", "Python 3 interpreter");
    kb.insert("pip", "Python package installer");
    kb.insert("pip3", "Python 3 package installer");
    kb.insert("node", "JavaScript runtime");
    kb.insert("npx", "Run npm packages binaries");
    kb.insert("pnpx", "Run pnpm packages binaries");
    kb.insert("yarnx", "Run yarn packages binaries");
    kb.insert("bun", "JavaScript runtime, bundler, and package manager");
    kb.insert("deno", "Secure JavaScript/TypeScript runtime");
    kb.insert("go", "Go programming language toolchain");
    kb.insert("go build", "Compile Go packages and dependencies");
    kb.insert("go test", "Run Go tests");
    kb.insert("go run", "Compile and run a Go program");
    kb.insert("go mod", "Module maintenance");
    kb.insert("rustc", "Rust compiler");
    kb.insert("rustup", "Rust toolchain installer and updater");
    kb.insert("cargo-watch", "Automatically run cargo when source code changes");

    kb
}

/// Returns a static map of common flags to their explanations.
fn flag_knowledge() -> HashMap<&'static str, &'static str> {
    let mut fk = HashMap::new();

    fk.insert("--release", "Build with optimizations (slower compile, faster binary)");
    fk.insert("--verbose", "Enable verbose output");
    fk.insert("--force", "Force the operation, bypassing safety checks");
    fk.insert("-f", "Force the operation");
    fk.insert("--help", "Show help information");
    fk.insert("-h", "Show help information (short form)");
    fk.insert("--version", "Show version information");
    fk.insert("-v", "Enable verbose output (short form)");
    fk.insert("-V", "Show version information (short form)");
    fk.insert("--watch", "Watch for file changes and re-run automatically");
    fk.insert("-w", "Watch for file changes (short form)");
    fk.insert("--debug", "Enable debug output or build in debug mode");
    fk.insert("-d", "Debug mode (short form)");
    fk.insert("-p", "Specify a package or port");
    fk.insert("--package", "Specify a package");
    fk.insert("--port", "Specify a port number");
    fk.insert("--output", "Specify the output file or directory");
    fk.insert("-o", "Specify output (short form)");
    fk.insert("--input", "Specify the input file");
    fk.insert("-i", "Specify input (short form) / interactive mode");
    fk.insert("--file", "Specify an input file");
    fk.insert("--all", "Apply to all items");
    fk.insert("-a", "Apply to all items (short form) / archive mode");
    fk.insert("--quiet", "Suppress normal output");
    fk.insert("-q", "Suppress normal output (short form)");
    fk.insert("--silent", "Suppress all output");
    fk.insert("--color", "Enable colored output");
    fk.insert("--no-color", "Disable colored output");
    fk.insert("--json", "Output in JSON format");
    fk.insert("--format", "Specify the output format");
    fk.insert("--target", "Specify the build target or triple");
    fk.insert("--features", "Enable specific features");
    fk.insert("--no-default-features", "Disable default features");
    fk.insert("--all-features", "Enable all features");
    fk.insert("--jobs", "Number of parallel jobs");
    fk.insert("-j", "Number of parallel jobs (short form)");
    fk.insert("--recursive", "Operate recursively");
    fk.insert("-r", "Operate recursively (short form)");
    fk.insert("--dry-run", "Show what would be done without making changes");
    fk.insert("-n", "Dry run (short form)");
    fk.insert("--yes", "Automatically answer yes to prompts");
    fk.insert("-y", "Automatically answer yes (short form)");
    fk.insert("--no", "Automatically answer no to prompts");
    fk.insert("--interactive", "Run in interactive mode");
    fk.insert("--offline", "Work offline without accessing the network");
    fk.insert("--locked", "Require Cargo.lock is up to date");
    fk.insert("--frozen", "Require Cargo.lock and cache are up to date");
    fk.insert("--message", "Set the commit message");
    fk.insert("-m", "Set the commit message (short form)");
    fk.insert("--all-match", "Apply to all matching items");
    fk.insert("--cached", "Use the cached/staged version");
    fk.insert("--ignore-case", "Case-insensitive matching");
    fk.insert("-i", "Case-insensitive matching (short form)");
    fk.insert("--line-number", "Show line numbers");
    fk.insert("-n", "Show line numbers (short form)");
    fk.insert("--count", "Show only a count of matching lines");
    fk.insert("-c", "Show count (short form)");
    fk.insert("--follow", "Follow symlinks");
    fk.insert("--hidden", "Include hidden files");
    fk.insert("--no-ignore", "Don't respect .gitignore files");
    fk.insert("--type", "Filter by file type");
    fk.insert("-t", "Filter by type (short form)");
    fk.insert("--glob", "Include/exclude files by glob pattern");
    fk.insert("-g", "Glob pattern (short form)");
    fk.insert("--invert-match", "Select non-matching lines");
    fk.insert("-v", "Invert match (short form)");
    fk.insert("--extended-regexp", "Use extended regular expressions");
    fk.insert("-E", "Extended regex (short form)");
    fk.insert("--pretty-print", "Pretty-print the output");
    fk.insert("--sort", "Sort output by the given field");
    fk.insert("--limit", "Limit the number of results");
    fk.insert("--detach", "Run container in background");
    fk.insert("-d", "Detach / daemon mode (short form)");
    fk.insert("--rm", "Automatically remove the container when it exits");
    fk.insert("--name", "Assign a name to the container");
    fk.insert("--env", "Set environment variables");
    fk.insert("-e", "Set environment variables (short form)");
    fk.insert("--volume", "Bind mount a volume");
    fk.insert("-v", "Volume mount (short form)");
    fk.insert("--network", "Connect a container to a network");
    fk.insert("--build", "Build images/services before starting");
    fk.insert("--no-build", "Don't build an image, even if it's missing");
    fk.insert("--tag", "Name and optionally tag the image");
    fk.insert("-t", "Tag the image (short form)");
    fk.insert("--file", "Specify the Dockerfile path");
    fk.insert("-f", "Dockerfile path (short form)");
    fk.insert("--build-arg", "Set build-time variables");
    fk.insert("--namespace", "Kubernetes namespace");
    fk.insert("-n", "Kubernetes namespace (short form)");
    fk.insert("--selector", "Selector (label query) to filter on");
    fk.insert("-l", "Label selector (short form)");
    fk.insert("--output", "Output format");
    fk.insert("-o", "Output format (short form)");
    fk.insert("--show-labels", "Show labels in the output");
    fk.insert("--wide", "Wide output (more columns)");
    fk.insert("-t", "Request TTY allocation (short form)");
    fk.insert("-it", "Interactive + TTY (pseudo-flag for docker exec / kubectl)");

    fk
}

// ── Tokenization ──────────────────────────────────────────────────────

/// Tokenize a shell command string, respecting quotes.
///
/// Returns a list of `(token, is_pipe, is_redirect, is_operator)` tuples.
fn tokenize(cmd: &str) -> Vec<TokenInfo> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let chars: Vec<char> = cmd.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];

        // Handle quote state
        if in_single_quote {
            current.push(ch);
            if ch == '\'' {
                in_single_quote = false;
            }
            i += 1;
            continue;
        }

        if in_double_quote {
            current.push(ch);
            if ch == '"' {
                in_double_quote = false;
            }
            i += 1;
            continue;
        }

        match ch {
            '\'' => {
                in_single_quote = true;
                current.push(ch);
                i += 1;
            }
            '"' => {
                in_double_quote = true;
                current.push(ch);
                i += 1;
            }
            '\\' => {
                // Escape next character
                current.push(ch);
                if i + 1 < chars.len() {
                    current.push(chars[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            '|' => {
                // Flush current token
                if !current.trim().is_empty() {
                    tokens.push(TokenInfo {
                        token: current.trim().to_string(),
                        kind: RawKind::Normal,
                    });
                    current.clear();
                }
                // Check for ||
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    tokens.push(TokenInfo {
                        token: "||".to_string(),
                        kind: RawKind::Operator,
                    });
                    i += 2;
                } else {
                    tokens.push(TokenInfo {
                        token: "|".to_string(),
                        kind: RawKind::Pipe,
                    });
                    i += 1;
                }
            }
            '&' => {
                if !current.trim().is_empty() {
                    tokens.push(TokenInfo {
                        token: current.trim().to_string(),
                        kind: RawKind::Normal,
                    });
                    current.clear();
                }
                // Check for &&
                if i + 1 < chars.len() && chars[i + 1] == '&' {
                    tokens.push(TokenInfo {
                        token: "&&".to_string(),
                        kind: RawKind::Operator,
                    });
                    i += 2;
                } else {
                    // Background & — treat as operator
                    tokens.push(TokenInfo {
                        token: "&".to_string(),
                        kind: RawKind::Operator,
                    });
                    i += 1;
                }
            }
            ';' => {
                if !current.trim().is_empty() {
                    tokens.push(TokenInfo {
                        token: current.trim().to_string(),
                        kind: RawKind::Normal,
                    });
                    current.clear();
                }
                tokens.push(TokenInfo {
                    token: ";".to_string(),
                    kind: RawKind::Operator,
                });
                i += 1;
            }
            '>' | '<' => {
                if !current.trim().is_empty() {
                    tokens.push(TokenInfo {
                        token: current.trim().to_string(),
                        kind: RawKind::Normal,
                    });
                    current.clear();
                }

                // Determine the full redirect token
                let redirect = if ch == '>' {
                    if i + 1 < chars.len() && chars[i + 1] == '>' {
                        // Check for 2>> or >>
                        if i + 2 < chars.len() && chars[i + 2] == '>' {
                            // 2>>> shouldn't happen, handle 2>> and >>
                            if !current.is_empty() {
                                // already flushed
                            }
                            let tok = format!(">>");
                            tokens.push(TokenInfo {
                                token: tok,
                                kind: RawKind::Redirect,
                            });
                            i += 2;
                            continue;
                        }
                        let tok = ">>".to_string();
                        tokens.push(TokenInfo {
                            token: tok,
                            kind: RawKind::Redirect,
                        });
                        i += 2;
                        continue;
                    } else {
                        ">".to_string()
                    }
                } else {
                    "<".to_string()
                };

                // Check for fd prefix (e.g., 2>, 2>>)
                // The digit was already flushed as a separate token.
                // Merge it with the redirect.
                let mut merged = false;
                if let Some(last) = tokens.last_mut() {
                    if last.kind == RawKind::Normal
                        && last.token.len() == 1
                        && last.token.chars().next().map_or(false, |c| c.is_ascii_digit())
                    {
                        // Check for fd redirect like 2>&1: after the > there may be &N
                        if redirect == ">" {
                            // Look ahead for & (e.g., 2>&1)
                            if i + 1 < chars.len() && chars[i + 1] == '&' {
                                let mut j = i + 2;
                                while j < chars.len() && chars[j].is_ascii_digit() {
                                    j += 1;
                                }
                                let fd_target: String = chars[i + 1..j].iter().collect();
                                last.token = format!("{}{}{}", last.token, redirect, fd_target);
                                i = j;
                                last.kind = RawKind::Redirect;
                                merged = true;
                            } else {
                                // no &N, just merge the fd prefix with redirect
                            }
                        } else {
                            // not a > redirect (e.g., < or >>)
                        }

                        if !merged {
                            last.token = format!("{}{}", last.token, redirect);
                            last.kind = RawKind::Redirect;
                            merged = true;
                        }
                    }
                }

                if !merged {
                    // Also check for plain >& (without fd prefix, e.g., >&2)
                    if redirect == ">" && i + 1 < chars.len() && chars[i + 1] == '&' {
                        let mut j = i + 2;
                        while j < chars.len() && chars[j].is_ascii_digit() {
                            j += 1;
                        }
                        let fd_target: String = chars[i + 1..j].iter().collect();
                        tokens.push(TokenInfo {
                            token: format!(">{}", fd_target),
                            kind: RawKind::Redirect,
                        });
                        i = j;
                    } else {
                        tokens.push(TokenInfo {
                            token: redirect,
                            kind: RawKind::Redirect,
                        });
                        i += 1;
                    }
                    continue;
                }
            }
            ' ' | '\t' => {
                if !current.trim().is_empty() {
                    tokens.push(TokenInfo {
                        token: current.trim().to_string(),
                        kind: RawKind::Normal,
                    });
                    current.clear();
                }
                i += 1;
            }
            _ => {
                current.push(ch);
                i += 1;
            }
        }
    }

    // Flush remaining
    if !current.trim().is_empty() {
        tokens.push(TokenInfo {
            token: current.trim().to_string(),
            kind: RawKind::Normal,
        });
    }

    tokens
}

#[derive(Debug, Clone)]
struct TokenInfo {
    token: String,
    kind: RawKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RawKind {
    Normal,
    Pipe,
    Redirect,
    Operator,
}

// ── Public API ────────────────────────────────────────────────────────

/// Explain a command string, returning structured parts.
///
/// Each part has a token, an explanation, and a kind.
pub fn explain_command(cmd: &str) -> Vec<ExplanationPart> {
    let tokens = tokenize(cmd);
    let kb = knowledge_base();
    let fk = flag_knowledge();

    let mut parts = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let info = &tokens[i];

        match info.kind {
            RawKind::Pipe => {
                parts.push(ExplanationPart {
                    token: info.token.clone(),
                    explanation: "pipes stdout of left command into stdin of right command"
                        .to_string(),
                    kind: TokenKind::Pipe,
                });
            }
            RawKind::Redirect => {
                let explanation = match info.token.as_str() {
                    ">" => "write stdout to file (overwrite)".to_string(),
                    ">>" => "append stdout to file".to_string(),
                    "<" => "read stdin from file".to_string(),
                    "2>" => "write stderr to file (overwrite)".to_string(),
                    "2>>" => "append stderr to file".to_string(),
                    "2>&1" => "redirect stderr to stdout".to_string(),
                    "1>&2" => "redirect stdout to stderr".to_string(),
                    ">&1" => "redirect stdout to file descriptor 1 (stdout, no-op)".to_string(),
                    ">&2" => "redirect stdout to stderr".to_string(),
                    "1>" => "write stdout to file (overwrite)".to_string(),
                    "1>>" => "append stdout to file".to_string(),
                    _ => format!("redirect: {}", info.token),
                };
                parts.push(ExplanationPart {
                    token: info.token.clone(),
                    explanation,
                    kind: TokenKind::Redirect,
                });
            }
            RawKind::Operator => {
                let explanation = match info.token.as_str() {
                    "&&" => "run next command only if previous succeeds".to_string(),
                    "||" => "run next command only if previous fails".to_string(),
                    ";" => "run next command regardless of previous result".to_string(),
                    "&" => "run command in the background".to_string(),
                    _ => format!("operator: {}", info.token),
                };
                parts.push(ExplanationPart {
                    token: info.token.clone(),
                    explanation,
                    kind: TokenKind::Operator,
                });
            }
            RawKind::Normal => {
                let token = &info.token;

                if i == 0 || parts.last().map_or(false, |p| {
                    matches!(p.kind, TokenKind::Pipe | TokenKind::Operator)
                }) {
                    // This is a binary — check for "binary subcommand" in KB
                    let binary = token.split_whitespace().next().unwrap_or(token);
                    let is_binary = if token.contains(' ') {
                        kb.contains_key(token.as_str())
                    } else {
                        kb.contains_key(token.as_str())
                    };

                    if is_binary {
                        let explanation = kb
                            .get(token.as_str())
                            .copied()
                            .unwrap_or("a known command");
                        parts.push(ExplanationPart {
                            token: token.clone(),
                            explanation: explanation.to_string(),
                            kind: TokenKind::Binary,
                        });
                    } else if kb.contains_key(binary) {
                        // The binary is known but the full "binary subcommand" is not
                        let sub_explanation = if token.contains(' ') {
                            let subcmd = &token[binary.len()..].trim();
                            format!("{} — unknown subcommand: {}", kb[binary], subcmd)
                        } else {
                            kb[binary].to_string()
                        };
                        parts.push(ExplanationPart {
                            token: token.clone(),
                            explanation: sub_explanation,
                            kind: TokenKind::Binary,
                        });
                    } else {
                        parts.push(ExplanationPart {
                            token: token.clone(),
                            explanation: format!("executable: {}", binary),
                            kind: TokenKind::Binary,
                        });
                    }

                    // If the token was just the binary (no subcommand), peek ahead
                    // to see if the next token is a subcommand
                    if !token.contains(' ') && i + 1 < tokens.len() {
                        if let Some(next) = tokens.get(i + 1) {
                            if next.kind == RawKind::Normal {
                                let combined = format!("{} {}", token, next.token);
                                if kb.contains_key(combined.as_str()) {
                                    let explanation = kb[combined.as_str()].to_string();
                                    // Replace the last part with the combined version
                                    parts.pop();
                                    parts.push(ExplanationPart {
                                        token: combined,
                                        explanation,
                                        kind: TokenKind::Binary,
                                    });
                                    i += 1; // skip the subcommand token
                                }
                            }
                        }
                    }
                } else {
                    // Not the first token after a pipe/operator — it's a flag or argument
                    if token.starts_with('-') || token.starts_with("--") {
                        // Check if it's a flag
                        let explanation = if let Some(&desc) = fk.get(token.as_str()) {
                            desc.to_string()
                        } else if token.starts_with("--") {
                            format!("flag: {}", token)
                        } else {
                            format!("short flag: {}", token)
                        };
                        parts.push(ExplanationPart {
                            token: token.clone(),
                            explanation,
                            kind: TokenKind::Flag,
                        });
                    } else {
                        parts.push(ExplanationPart {
                            token: token.clone(),
                            explanation: "argument".to_string(),
                            kind: TokenKind::Argument,
                        });
                    }
                }
            }
        }

        i += 1;
    }

    parts
}

/// Split a command on pipe characters (respecting quotes) and return each segment.
pub fn explain_pipes(cmd: &str) -> Vec<PipeSegment> {
    let segments = split_on_pipes(cmd);
    segments
        .into_iter()
        .enumerate()
        .map(|(pos, command)| PipeSegment { command, position: pos })
        .collect()
}

/// Split a command string on unquoted pipe characters.
fn split_on_pipes(cmd: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    for ch in cmd.chars() {
        if in_single_quote {
            current.push(ch);
            if ch == '\'' {
                in_single_quote = false;
            }
            continue;
        }
        if in_double_quote {
            current.push(ch);
            if ch == '"' {
                in_double_quote = false;
            }
            continue;
        }
        match ch {
            '|' => {
                segments.push(current.trim().to_string());
                current.clear();
            }
            '\'' => {
                in_single_quote = true;
                current.push(ch);
            }
            '"' => {
                in_double_quote = true;
                current.push(ch);
            }
            '\\' => {
                current.push(ch);
                // next char will be appended in the next iteration
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.trim().is_empty() {
        segments.push(current.trim().to_string());
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_cargo_build_release() {
        let parts = explain_command("cargo build --release");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].token, "cargo build");
        assert_eq!(parts[0].kind, TokenKind::Binary);
        assert!(parts[0].explanation.contains("Compile"));
        assert_eq!(parts[1].token, "--release");
        assert_eq!(parts[1].kind, TokenKind::Flag);
        assert!(parts[1].explanation.contains("optimization"));
    }

    #[test]
    fn test_explain_piped_command() {
        let parts = explain_command("cargo test 2>&1 | rg failed");
        // Should contain pipe, binary, redirect, binary parts
        let kinds: Vec<&TokenKind> = parts.iter().map(|p| &p.kind).collect();
        assert!(kinds.contains(&&TokenKind::Binary));
        assert!(kinds.contains(&&TokenKind::Pipe));
        assert!(kinds.contains(&&TokenKind::Redirect));
    }

    #[test]
    fn test_explain_operators() {
        let parts = explain_command("cargo build && cargo test");
        assert!(parts.iter().any(|p| p.token == "&&"));
        let and_part = parts.iter().find(|p| p.token == "&&").unwrap();
        assert!(and_part.explanation.contains("succeeds"));
    }

    #[test]
    fn test_explain_git_commands() {
        let parts = explain_command("git add -A");
        assert_eq!(parts[0].token, "git add");
        assert_eq!(parts[0].kind, TokenKind::Binary);
        assert!(parts[0].explanation.contains("Stage"));
    }

    #[test]
    fn test_explain_redirects() {
        let parts = explain_command("echo hello >> output.txt");
        assert!(parts.iter().any(|p| p.token == ">>"));
        let redir = parts.iter().find(|p| p.token == ">>").unwrap();
        assert!(redir.explanation.contains("append"));
        assert_eq!(redir.kind, TokenKind::Redirect);
    }

    #[test]
    fn test_explain_pipes_function() {
        let segments = explain_pipes("find . -name '*.rs' | head -10 | sort");
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].position, 0);
        assert_eq!(segments[0].command, "find . -name '*.rs'");
        assert_eq!(segments[1].command, "head -10");
        assert_eq!(segments[2].position, 2);
    }

    #[test]
    fn test_tokenizer_respects_quotes() {
        let tokens = tokenize("echo 'hello | world'");
        // The pipe inside quotes should NOT be treated as a pipe
        let pipe_count = tokens.iter().filter(|t| t.kind == RawKind::Pipe).count();
        assert_eq!(pipe_count, 0);
        // There should be exactly 2 normal tokens: "echo" and "'hello | world'"
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_explain_docker_compose() {
        let parts = explain_command("docker-compose up --build -d");
        assert_eq!(parts[0].token, "docker-compose up");
        assert_eq!(parts[0].kind, TokenKind::Binary);
    }

    #[test]
    fn test_explain_unknown_command() {
        let parts = explain_command("my-custom-tool --flag value");
        assert_eq!(parts[0].kind, TokenKind::Binary);
        assert!(parts[0].explanation.contains("my-custom-tool"));
    }
}