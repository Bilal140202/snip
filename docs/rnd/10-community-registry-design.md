# Community Registry Design

> Feature Agent #10 | Design Document
> Depends on: `03-tldr-analysis.md` (community content model)
> Status: Phase 2 design (no server required for MVP)

---

## 1. The Registry Concept

### What it is

A **community snippet pack registry** — like tldr-pages but for *project* commands, not system commands.

| | tldr | snip registry |
|---|---|---|
| **Unit** | Single command page | Pack of related snippets |
| **Scope** | Standard CLI tools (`tar`, `git`) | Project/framework commands (`next dev`, `rails db:migrate`) |
| **Content source** | One monolithic GitHub repo | Many independent GitHub repos |
| **Author** | Community contributors | Anyone (namespaced by GitHub org/user) |
| **Distribution** | Zip from GitHub Releases | Git clone or GitHub API fetch |
| **Trust model** | Maintainer PR review | Namespace + stars + optional signing |

### User-facing commands

```bash
# Search for packs
$ snip pack search nextjs
  official/nextjs     ★2.3k  Next.js project commands
  vercel/next-utils   ★340   Extra Vercel/Next utility snippets

# Install a pack (merges .snips into your project)
$ snip pack add official/nextjs
  ⚡ Adding official/nextjs v1.2.0...
  ⚡ Added 12 snippets from official/nextjs
  ⚡ Run `snip` to see them.

# List installed packs
$ snip pack list
  official/nextjs  v1.2.0  (12 snippets)
  company/internal v0.3.0  (8 snippets)

# Update installed packs
$ snip pack update
  ⚡ official/nextjs  v1.2.0 → v1.3.0  (+3 snippets)
  ⚡ company/internal — already up to date

# Remove a pack
$ snip pack remove official/nextjs
  ⚡ Removed official/nextjs (12 snippets removed)

# Show pack info
$ snip pack info official/nextjs
  Name:        official/nextjs
  Version:     1.2.0
  Description: Official Next.js snippet pack
  Snippets:    12
  Author:      snip-official
  Repo:        github.com/snip-packs/nextjs
  Stars:       2,341
```

### Key design principle: Packs are just `.snips` files in GitHub repos

There is no special build step, no compilation, no packaging tool. A pack is literally:

1. A GitHub repo with a `snippack.toml` manifest
2. One or more `.snips` files in that repo
3. Optionally a GitHub release with a `snips.zip`

That's it. GitHub *is* the registry. GitHub *is* the CDN. GitHub *is* the auth system.

---

## 2. Pack Format

### The Manifest: `snippack.toml`

Every pack repo must have a `snippack.toml` at its root:

```toml
[pack]
name = "nextjs"
version = "1.2.0"
description = "Official Next.js snippet pack — dev, build, deploy, and debugging commands"
author = "snip-official"
repo = "https://github.com/snip-packs/nextjs"
license = "MIT"
keywords = ["nextjs", "react", "vercel", "frontend"]

[pack.frameworks]
# Tags for search/discovery. Not enforced — purely metadata.
tags = ["nextjs", "react"]

[pack.snips]
# Which .snips files to include and where they come from
# If omitted, all .snips files in repo root are included
files = ["nextjs.snips"]

# Optional: prefix all snippet groups from this pack
# Prevents collisions when multiple packs define "dev" or "build"
group_prefix = "nextjs"

[pack.compatibility]
snip_version = ">=0.2.0"
```

### Manifest fields explained

| Field | Required | Description |
|-------|----------|-------------|
| `pack.name` | **Yes** | Short identifier. Used as `{author}/{name}` in the registry. |
| `pack.version` | **Yes** | Semver string. Used for update checks and `.snips.lock`. |
| `pack.description` | **Yes** | One-line description shown in search results. |
| `pack.author` | **Yes** | GitHub username or org. Forms the namespace prefix. |
| `pack.repo` | No | Auto-derived from `github:{author}/{name}` if omitted. |
| `pack.license` | No | SPDX identifier. Shown in `pack info`. |
| `pack.keywords` | No | Additional search terms. |
| `pack.frameworks.tags` | No | Framework tags for filtered search. |
| `pack.snips.files` | No | Glob patterns for `.snips` files. Default: `*.snips`. |
| `pack.snips.group_prefix` | No | Prefix all groups to avoid name collisions. |
| `pack.compatibility.snip_version` | No | Minimum snip version required. |

### Example repo structure: `snip-packs/nextjs`

```
snip-packs/nextjs/
├── snippack.toml          # Manifest
├── README.md              # Human docs (rendered by GitHub)
├── nextjs.snips           # The actual snippets
└── .github/
    └── FUNDING.yml        # Optional: sponsor link
```

### Example `nextjs.snips`

```yaml
# nextjs.snips — Official Next.js snippet pack
# Pack: official/nextjs v1.2.0

dev:
  description: Start development server
  command: pnpm dev
  tags: [nextjs, dev]

build:
  description: Create production build
  command: pnpm build
  tags: [nextjs, build]

start:
  description: Start production server
  command: pnpm start
  tags: [nextjs, production]

lint:
  description: Run ESLint
  command: pnpm lint
  tags: [nextjs, lint]

type-check:
  description: Run TypeScript type checking
  command: pnpm tsc --noEmit
  tags: [nextjs, typescript]

db-migrate:
  description: Run database migrations (Prisma)
  command: pnpm prisma migrate dev
  tags: [nextjs, database, prisma]

db-studio:
  description: Open Prisma Studio (database GUI)
  command: pnpm prisma studio
  tags: [nextjs, database, prisma]

generate:
  description: Generate Next.js component/page/api route
  command: pnpm dlx @snip/generator nextjs {{type}} {{name}}
  tags: [nextjs, scaffold]
  args:
    - name: type
      description: "Component type (component, page, api, layout)"
      required: true
    - name: name
      description: "Name for the generated file"
      required: true
```

### Multi-file packs

For larger packs, split across multiple `.snips` files:

```
snip-packs/rails/
├── snippack.toml
├── core.snips              # db, server, console
├── testing.snips           # rspec, factory_bot
├── deployment.snips        # capistrano, heroku
└── generators.snips        # rails generate commands
```

```toml
# snippack.toml
[pack.snips]
files = ["core.snips", "testing.snips", "deployment.snips", "generators.snips"]
```

---

## 3. Registry API

### The two-phase approach

We design the *full* API here, but the MVP (Phase 2) implements **none of it as a server**. Instead, Phase 2 uses GitHub directly as the registry (see §6).

### Phase 3: Dedicated registry server (future)

Could be hosted on **GitHub Pages for free** — it's just a static JSON index rebuilt on a cron or GitHub Action.

#### `GET /api/packs?search={query}&framework={tag}&limit={n}`

Search packs by name, description, keywords, and framework tags.

**Response:**
```json
{
  "packs": [
    {
      "author": "official",
      "name": "nextjs",
      "version": "1.2.0",
      "description": "Official Next.js snippet pack",
      "stars": 2341,
      "snippet_count": 12,
      "updated_at": "2025-07-08T14:30:00Z"
    },
    {
      "author": "vercel",
      "name": "next-utils",
      "version": "0.5.0",
      "description": "Extra Vercel/Next utility snippets",
      "stars": 340,
      "snippet_count": 8,
      "updated_at": "2025-07-01T09:00:00Z"
    }
  ],
  "total": 2,
  "query": "nextjs"
}
```

#### `GET /api/packs/{author}/{name}`

Get full pack metadata including all snippet names and descriptions (not full content).

**Response:**
```json
{
  "pack": {
    "author": "official",
    "name": "nextjs",
    "version": "1.2.0",
    "description": "Official Next.js snippet pack",
    "license": "MIT",
    "stars": 2341,
    "repo": "https://github.com/snip-packs/nextjs",
    "snip_version_req": ">=0.2.0",
    "files": ["nextjs.snips"],
    "group_prefix": "nextjs",
    "updated_at": "2025-07-08T14:30:00Z",
    "snippets": [
      {"group": "dev", "description": "Start development server"},
      {"group": "build", "description": "Create production build"},
      {"group": "db-migrate", "description": "Run database migrations (Prisma)"}
    ]
  }
}
```

#### `GET /api/packs/{author}/{name}/snips`

Get the full `.snips` file content. Returns raw text, not JSON.

```
HTTP 200
Content-Type: text/yaml

# nextjs.snips — Official Next.js snippet pack
dev:
  description: Start development server
  command: pnpm dev
...
```

Alternatively, for packs distributed as release zips:

```
HTTP 200
Content-Type: application/zip
Content-Disposition: attachment; filename="nextjs-1.2.0.snips.zip"
```

#### `POST /api/packs` — Publish

Register/update a pack in the index. **Authentication: GitHub token.**

**Request:**
```json
{
  "repo": "https://github.com/myuser/my-snippet-pack",
  "token": "ghp_xxxx"
}
```

The server then:
1. Validates the token against GitHub API
2. Clones the repo temporarily
3. Validates `snippack.toml` schema
4. Validates all referenced `.snips` files parse correctly
5. Fetches star count from GitHub API
6. Adds/updates the pack in the JSON index
7. Commits and pushes (triggers Pages rebuild)

#### `GET /api/packs/{author}/{name}/versions`

List all available versions (from GitHub releases/tags).

**Response:**
```json
{
  "versions": [
    {"version": "1.2.0", "tag": "v1.2.0", "date": "2025-07-08"},
    {"version": "1.1.0", "tag": "v1.1.0", "date": "2025-06-15"},
    {"version": "1.0.0", "tag": "v1.0.0", "date": "2025-05-01"}
  ]
}
```

### Static JSON index (GitHub Pages approach)

For the free hosting model, the entire registry is a single JSON file:

```
snip-registry/
├── index.json              # Full pack index
├── packs/
│   ├── official/
│   │   ├── nextjs.json     # Pack metadata
│   │   └── rust.json
│   └── community/
│       ├── ...             # Auto-generated from submissions
└── _config.yml             # GitHub Pages config
```

`index.json` (~100KB for 500 packs, perfectly cacheable):
```json
{
  "updated": "2025-07-09T00:00:00Z",
  "packs": {
    "official/nextjs": {
      "author": "official",
      "name": "nextjs",
      "version": "1.2.0",
      "description": "Official Next.js snippet pack",
      "stars": 2341,
      "snippet_count": 12,
      "repo": "snip-packs/nextjs",
      "updated_at": "2025-07-08T14:30:00Z"
    }
  }
}
```

A GitHub Action rebuilds this index nightly by crawling all repos with the `snip-pack` GitHub topic.

---

## 4. Verification & Trust

### The trust hierarchy

```
┌─────────────────────────────────┐
│  official/*                     │  ← Vetted by snip core team
│  (curated, reviewed, tested)    │
├─────────────────────────────────┤
│  verified/*                     │  ← Known orgs (Vercel, Rails, etc.)
│  (org-validated identity)       │
├─────────────────────────────────┤
│  {user}/*                       │  ← Community packs
│  (star count as quality signal) │
└─────────────────────────────────┘
```

### Namespace rules

| Namespace | Who can publish | Trust level |
|-----------|----------------|-------------|
| `official/` | snip core maintainers only | Highest — reviewed, tested, pinned in docs |
| `verified/` | Orgs with GitHubverified domain | High — real company/org identity |
| `{username}/` | Anyone with a GitHub account | Variable — stars + download count signal quality |

### Star count as quality signal

Stars are the *primary* quality metric for community packs. Displayed prominently:

```
$ snip pack search rails
  official/rails      ★5.1k  Official Ruby on Rails commands
  driftingruby/rails  ★890   Drifting Ruby supplementary snippets
  josh/rails-extra    ★12    Personal Rails snippets
```

The threshold for showing in default search: **≥ 5 stars**. Below that, only shown with `--all` flag. This prevents spam/garbage from polluting results while keeping the barrier to entry low.

### Pack signing (Phase 4, optional/advanced)

For teams that need supply chain security:

1. Pack author generates a keypair: `snip pack keygen`
2. Signs the pack: `snip pack sign` → creates `snippack.sig`
3. Consumers verify: `snip pack add --verify official/nextjs`

Implementation: Ed25519 signatures over the SHA256 of all `.snips` file content concatenated in manifest order. The public key is stored in the pack's GitHub repo (or a separate trust store).

**This is explicitly out of scope for MVP but the manifest format reserves space for it:**

```toml
[pack.verify]
public_key = "https://github.com/snip-packs/nextjs/blob/main/snippack.pub"
signature = "snippack.sig"
```

### The `.snips.lock` file

Pinned pack versions, committed to the project repo alongside `.snip.yml`:

```toml
# .snips.lock — Auto-generated. Do not edit manually.
# Run `snip pack update` to update versions.

[[packs]]
author = "official"
name = "nextjs"
version = "1.2.0"
source = "github:snip-packs/nextjs"
commit = "a1b2c3d4e5f6"
fetched_at = "2025-07-08T14:30:00Z"
files = ["nextjs.snips"]
snippet_count = 12
checksum = "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"

[[packs]]
author = "company"
name = "internal-tools"
version = "0.3.0"
source = "github:company/snip-internal"
commit = "f6e5d4c3b2a1"
fetched_at = "2025-07-05T10:00:00Z"
files = ["infra.snips", "deploy.snips"]
snippet_count = 8
checksum = "sha256:60303ae22b998861bce3b28f33ece1e6990497b8a1e6b3ee8b6d8a4b0d4e2a1c"
```

**Why `.snips.lock` matters:**

1. **Reproducibility** — Every team member gets the same snippet versions. `snip pack install` (no args) reads the lockfile.
2. **Audit trail** — `git log .snips.lock` shows when packs were added/updated.
3. **Offline resilience** — Snip falls back to the cached/locked version when offline (same pattern as tlrc).
4. **Security** — Pinned commit SHAs prevent supply chain attacks via pack updates.
5. **CI/CD** — CI runs `snip pack install --frozen` which errors if lockfile is stale.

### Update workflow

```
$ snip pack update
  ⚡ Checking for updates...
  ⚡ official/nextjs  v1.2.0 → v1.3.0  (+3 snippets, -1 removed)
  ⚡ company/internal — already up to date
  ⚡
  ⚡ Update .snips.lock? [Y/n]
  ⚡
  ⚡ Updated. Run `git add .snips.lock && git commit -m 'update snippet packs'`
```

---

## 5. The Viral Loop

### How the registry drives adoption

```
Step 1: Individual use
  Developer A runs `snip` in their Next.js project.
  Sees: "No community packs installed. Try: snip pack search nextjs"
  Installs official/nextjs. Gets 12 useful commands immediately.
  Thinks: "This is great."

Step 2: Team sharing
  Developer A adds `official/nextjs` to their project.
  Commits .snips.lock. Team pulls, runs `snip pack install`.
  Everyone has the same commands. Onboarding new devs: "just run snip."

Step 3: Company internal pack
  Developer A realizes their company has 30 custom scripts.
  Creates `company/internal-tools` pack with:
    - infra.snips (Terraform, kubectl, helm commands)
    - deploy.snips (company deployment pipeline)
    - db.snips (company database tools)
  Adds it to the monorepo. 50 devs now share the same command knowledge.

Step 4: Cross-team discovery
  Team B sees Team A's pack in the monorepo.
  Forks it, adds their own snippets.
  Publishes as `company-frontend/react-utils`.
  Both teams benefit from each other's work.

Step 5: Ecosystem growth
  Developer A publishes `official/nextjs` publicly.
  Community contributes PRs with more snippets.
  Other developers create packs: vercel/next-utils, timneutkens/next-advanced.
  Search "nextjs" returns 15 packs. The ecosystem is self-sustaining.

Step 6: The flywheel
  More packs → more `snip` users → more pack authors → more packs.
  Like npm, but the "dependencies" are *knowledge*, not code.
```

### Why this is different from npm/homebrew

| | npm | homebrew | snip packs |
|---|---|---|---|
| **What you share** | Code libraries | CLI tools | Command knowledge |
| **Installation** | Adds to node_modules | Adds to /usr/local | Merges into .snips |
| **Conflict model** | Version ranges | Bottle conflicts | Group prefixes |
| **Commit to repo?** | No (node_modules gitignored) | No | **Yes** (.snips.lock) |
| **Team alignment** | package.json | Brewfile | .snips.lock |
| **Learning curve** | High (semver, peer deps) | Medium | **Zero** (it's just YAML) |

The key insight: **snip packs don't add dependencies. They add *knowledge*.** There's no version conflict, no runtime dependency, no build step. Installing a pack just means "I now know these commands exist."

### The "cheat sheet" effect

When someone installs their first pack and sees:

```
$ snip
  Next.js Commands (from official/nextjs):
    dev          Start development server
    build        Create production build
    db-migrate   Run database migrations (Prisma)
    ...
```

They immediately understand the value. It's a living, project-aware cheat sheet that their whole team shares. This is the moment of conversion.

---

## 6. MVP Scope — Phase 2 (Zero Infrastructure)

### The critical insight: GitHub IS the registry

We don't need a server. We don't need a database. We don't need a CI pipeline.

**GitHub already provides everything we need:**

| Need | GitHub Feature |
|------|---------------|
| Search | GitHub API: `GET /search/repositories?q=topic:snip-pack+nextjs` |
| Download | Git clone or GitHub raw file download |
| Auth | `gh auth status` / `GITHUB_TOKEN` env var |
| Trust | Stars, org verification, repo age |
| Versioning | Git tags / GitHub releases |
| Namespacing | `{org}/{repo}` is already the namespace |
| Caching | `~/.cache/snip/packs/{author}/{name}/` |
| Discovery | GitHub Topics (`snip-pack`) |

### MVP command: `snip pack add <github-url>`

The core command. Takes a GitHub URL and merges snippets into the project.

```rust
// Pseudocode for the MVP implementation
fn pack_add(github_url: &str) -> Result<()> {
    // 1. Parse the URL
    //    Accepts: "github:user/repo", "user/repo", "https://github.com/user/repo"
    let (owner, repo) = parse_github_url(github_url)?;

    // 2. Find .snips files in the repo (via GitHub API, no clone needed)
    //    GET /repos/{owner}/{repo}/contents/
    //    Filter for *.snips files
    let snips_files = github_api::list_snips_files(&owner, &repo)?;

    // 3. Download each .snips file (raw content)
    //    GET /repos/{owner}/{repo}/contents/{path}?ref=main
    for file in &snips_files {
        let content = github_api::download_file(&owner, &repo, file, "main")?;
        merge_snips_into_project(&content)?;
    }

    // 4. Optionally fetch snippack.toml for metadata
    let manifest = github_api::download_file(&owner, &repo, "snippack.toml", "main")
        .ok()
        .and_then(|c| toml::from_str::<SnipPackManifest>(&c).ok());

    // 5. Update .snips.lock
    update_lockfile(&owner, &repo, &manifest, &snips_files)?;

    Ok(())
}
```

**URL formats accepted:**

```bash
# All of these resolve to the same pack:
snip pack add github:snip-packs/nextjs
snip pack add snip-packs/nextjs
snip pack add https://github.com/snip-packs/nextjs
```

**What happens to the snippets:**

1. Downloaded `.snips` files are stored in `~/.cache/snip/packs/{owner}/{repo}/`
2. Snippet definitions are **merged** into the project's existing `.snip.yml` (or a new one)
3. Each merged snippet is tagged with `_pack: "{owner}/{repo}"` for tracking
4. `.snips.lock` is created/updated with pack metadata and checksums

**Merge conflict resolution:**

When a pack defines a group name that already exists locally:

```
⚠ Group "build" already exists in your .snip.yml
  Local:  pnpm build
  Pack:   pnpm build --profile production
  [s]kip  [o]verwrite  [r]ename (build → nextjs-build)  [S]how both
```

Default: **rename** with the pack's `group_prefix`. This is the safest default and matches how npm handles scoped packages.

### MVP command: `snip pack search <query>`

Search GitHub for repos with the `snip-pack` topic.

```rust
fn pack_search(query: &str) -> Result<()> {
    // GitHub Search API
    // GET /search/repositories?q={query}+topic:snip-pack&sort=stars&order=desc
    let url = format!(
        "https://api.github.com/search/repositories?q={}+topic:snip-pack&sort=stars&order=desc&per_page=20",
        urlencoding::encode(query)
    );

    let response: GithubSearchResponse = ureq::get(&url)
        .set("Accept", "application/vnd.github.v3+json")
        .set("User-Agent", "snip-cli")
        .call()?
        .into_json()?;

    // Render results
    for repo in &response.items {
        println!(
            "  {}/{}  ★{}  {}",
            style(repo.owner.login).cyan(),
            style(repo.name).green(),
            style(repo.stargazers_count).yellow(),
            repo.description.unwrap_or_default().truncate(60)
        );
    }

    Ok(())
}
```

**Example output:**

```
$ snip pack search nextjs
  ⚡ Searching GitHub for "nextjs" packs...

  snip-packs/nextjs        ★2.3k  Official Next.js snippet pack — dev, build, deploy
  vercel/next-utils         ★340   Extra Vercel/Next utility snippets
  timneutkens/next-advanced  ★89   Advanced Next.js patterns and commands
  lee/next-snippets           ★23   Personal Next.js snippets

  4 packs found. Install: snip pack add <owner/repo>
```

### MVP command: `snip pack list`

Show installed packs from `.snips.lock`:

```bash
$ snip pack list
  official/nextjs   v1.2.0  (12 snippets)  cached
  company/internal  v0.3.0  (8 snippets)   cached
```

### MVP command: `snip pack update`

Check each installed pack for new commits/tags:

```rust
fn pack_update() -> Result<()> {
    let lockfile = SnipsLock::load()?;
    for pack in &lockfile.packs {
        // Check if there's a newer commit on the default branch
        let latest = github_api::latest_commit(&pack.author, &pack.name)?;
        if latest.sha != pack.commit {
            println!("  {}/{}  {} → {} (new commits available)",
                pack.author, pack.name, &pack.commit[..7], &latest.sha[..7]);
        }
    }
    Ok(())
}
```

### MVP command: `snip pack remove <author/name>`

Remove a pack and all its snippets:

```rust
fn pack_remove(author: &str, name: &str) -> Result<()> {
    // 1. Read .snips.lock to find the pack
    // 2. Read .snip.yml, remove all entries tagged with _pack: "{author}/{name}"
    // 3. Remove pack from .snips.lock
    // 4. Optionally clear cache: ~/.cache/snip/packs/{author}/{name}/
    Ok(())
}
```

### What MVP does NOT include

| Feature | Phase | Why deferred |
|---------|-------|-------------|
| Dedicated registry server | Phase 3 | GitHub API is sufficient for < 10K packs |
| Pack signing/verification | Phase 4 | Ed25519 adds complexity; lockfile SHAs are good enough for now |
| `verified/` namespace | Phase 3 | Needs org verification workflow |
| Pack dependencies (packs depending on other packs) | Never (by design) | Packs are independent. No dependency hell. |
| Private pack registries | Phase 3 | GitHub private repos already work with `snip pack add` |
| Web UI for browsing packs | Phase 4 | CLI-first. Web is nice-to-have. |
| Pack version ranges (semver constraints) | Phase 3 | MVP pins to exact commits. Semver comes with the registry. |

### Implementation checklist for MVP

```
[ ] Parse GitHub URL formats (github:user/repo, user/repo, https://...)
[ ] GitHub API client (list files, download raw content, search repos)
[ ] GITHUB_TOKEN detection (gh auth, env var, git config)
[ ] snippack.toml parser (validate manifest schema)
[ ] .snips file merger (handle conflicts via group_prefix or prompt)
[ ] .snips.lock read/write (TOML)
[ ] Pack cache at ~/.cache/snip/packs/{author}/{name}/
[ ] snip pack add <github-url>
[ ] snip pack search <query>
[ ] snip pack list
[ ] snip pack update
[ ] snip pack remove <author/name>
[ ] snip pack install (reads .snips.lock, installs all)
[ ] snip pack install --frozen (errors if lockfile is stale)
[ ] Offline fallback (use cached packs when GitHub is unreachable)
```

### Dependencies for MVP

```toml
# Cargo.toml additions
ureq = { version = "3", features = ["json"] }   # HTTP client (GitHub API)
toml = "1"                                        # snippack.toml + .snips.lock parsing
serde = { version =1", features = ["derive"] }    # API response deserialization
serde_json = "1"                                  # GitHub API JSON responses
urlencoding = "2"                                 # Search query encoding
```

No new major dependencies beyond what tlrc already uses. The `ureq` crate is already recommended in the tldr analysis.

---

## Appendix: CLI Interface Summary

```
snip pack add <github-url>        Install a pack from GitHub
snip pack search <query>          Search GitHub for packs
snip pack list                    Show installed packs
snip pack update                  Check for pack updates
snip pack remove <author/name>    Remove an installed pack
snip pack install                 Install all packs from .snips.lock
snip pack install --frozen        Install from lockfile, error if stale
snip pack info <author/name>      Show pack metadata
```

All `pack` subcommands are behind the `pack` keyword — no namespace pollution with existing `snip` commands. The `pack` keyword was chosen over `registry` because it's shorter, more actionable, and mirrors `npm pack` / `cargo package` conventions.

---

## Appendix: Data Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Developer's Machine                       │
│                                                             │
│  .snip.yml              .snips.lock                         │
│  ┌──────────┐          ┌──────────────┐                     │
│  │ local    │  ◄────  │ pack pins    │                     │
│  │ snippets │  merge  │ (commits,    │                     │
│  │          │         │  checksums)  │                     │
│  └──────────┘          └──────┬───────┘                     │
│       ▲                       │                             │
│       │ snip                  │ snip pack install            │
│       │                       ▼                             │
│  ┌──────────┐          ┌──────────────┐                     │
│  │ merged   │          │ pack cache   │                     │
│  │ view     │          │ ~/.cache/    │                     │
│  └──────────┘          │  snip/packs/ │                     │
│                        └──────┬───────┘                     │
│                               │                             │
└───────────────────────────────┼─────────────────────────────┘
                                │
                    GitHub API  │  git clone / raw download
                                │
                        ┌───────▼───────┐
                        │   GitHub      │
                        │               │
                        │ snip-packs/   │
                        │   nextjs/     │
                        │     snippack  │
                        │     .toml     │
                        │     nextjs    │
                        │     .snips    │
                        │               │
                        │ topic:        │
                        │ snip-pack     │
                        └───────────────┘
```