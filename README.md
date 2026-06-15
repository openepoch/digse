# Digse

**Digse** = **Dig Search Engines** — a lightweight, privacy-focused metasearch
engine CLI written in Rust. Digse aggregates results from many search engines
at once, deduplicates and ranks them, and serves them through a local HTTP API
(always structured JSON) plus a small built-in web UI.

## Install

One command. No build step. No admin or sudo required.

**Linux**
```bash
curl -fsSL https://raw.githubusercontent.com/openepoch/digse/main/docs/install.sh | sh
```

**Windows** (PowerShell)
```powershell
irm https://raw.githubusercontent.com/openepoch/digse/main/docs/install.ps1 | iex
```

This drops a static `digse` binary into `~/.local/bin` (Linux) or
`%LOCALAPPDATA%\digse` (Windows) and puts it on your PATH. The
[install page][install] has per-platform download links and the full scripts.

[install]: https://openepoch.github.io/digse/

## Quick start

```bash
digse start                                            # start the daemon at http://127.0.0.1:8888
digse version                                          # version + build target

# structured JSON straight from the local API
curl -s 'http://127.0.0.1:8888/search?q=what+is+rust&count=5' | jq '.results[].title'

# or open the built-in search UI in a browser
xdg-open http://127.0.0.1:8888   # Linux

digse ps                                               # is it running? (pid + url)
digse stop                                             # stop it
digse startup add                                      # start at every boot (no admin)
```

## Features

- **🔍 206 engines** across 12 categories (198 enabled by default)
- **🌐 Local HTTP API** — `digse start` runs a daemon serving `GET /search?q=...` returning a rich JSON envelope
- **🖥️ Built-in web UI** — `GET /` is a textbox search page that renders results inline
- **⚙️ Persisted config** — `digse config` stores search defaults **and** serve settings at `~/.digse/config.toml`
- **🚀 Cross-platform daemon** — Linux and Windows, with boot autostart and **no administrator/sudo required**
- **🎯 14 result types** — web, images, videos, music, news, files, torrents, academic, it, social, maps, shopping, weather, all
- **🔧 Fine-grained control** — engine selection, concurrency, timeouts, rate limiting, retries
- **🌍 URL-based filtering** — filetype/include/exclude filters applied to result URLs, no extra requests
- **📦 Single static binary** — prebuilt releases for 5 target triples, installable with a one-liner
- **🏗️ Modular workspace** — separate `core`, `engines`, `http`, `parser`, and `cli` crates
- **📚 Library** — usable as a Rust library

## Build from source

Prefer to build it yourself? Digse is a standard Cargo workspace.

```bash
git clone https://github.com/openepoch/digse.git
cd digse
cargo install --path crates/cli        # installs `digse` into ~/.cargo/bin
```

Or build and run without installing:

```bash
cargo run -q -p digse -- start
```

## Commands

```
digse start [--host H] [--port P]    start the server as a background daemon
digse restart [--host H] [--port P]  restart the running server (or start it)
digse stop                           stop the running server
digse ps                             report whether the server is running
digse version                        version + build target (--version is the terse form)
digse startup status|add|remove      manage boot-time autostart
digse config <sub>                   view or change persisted settings
digse list engines|categories        enumerate engines / categories
```

`--host`/`--port` override `serve.host`/`serve.port` for that run. The daemon
re-execs itself fully detached (a new session via `setsid` on Unix, no console
window on Windows) and tracks its PID in `~/.digse/start.pid`.

## The daemon

`digse start` spawns the server detached and waits for it to bind before
returning. Logs append to `~/.digse/start.log`; if the port is in use, `start`
fails fast with a pointer to that log.

```bash
digse start --host 0.0.0.0 --port 9000   # bind a different interface/port
digse restart                            # stop + start (picks up config changes)
```

`stop` is graceful-first (SIGTERM on Unix, `TerminateProcess` on Windows) and
idempotent — it exits 0 even if nothing is running.

## HTTP API

No external HTTP framework is used — just tokio.

| Method & path | Description |
| --- | --- |
| `GET /` | HTML search page (textbox + options; JS calls `/search`) |
| `GET /search?q=...` | JSON search envelope (200), or a JSON error |
| `GET /config` | HTML settings form |
| `POST /config` | save settings (redirects back to the form) |
| `GET /health` | `{"status":"ok"}` |
| other `GET` | `404` JSON |
| other methods | `405` JSON |

### `GET /search` parameters

| Param | Description |
| --- | --- |
| `q` (required) | the query |
| `type` | result type (default `web`) |
| `count` | results per engine (default: config `search.count`, else 10) |
| `offset` | pagination offset (default 0) |
| `total_results` | ceiling on total results returned (default: config, else 20) |
| `timeout` | per-engine request timeout in seconds (default: config, else 5) |
| `concurrent_engines` | engines queried at once (default: config, else 12) |
| `engines` | use only these engines (comma list) |
| `exclude_engines` | exclude these engines (comma list) |
| `categories` | use engines in these categories (comma list) |
| `language` | language preference |
| `time_range` | `day`, `week`, `month`, or `year` |
| `safe_search` | `1` to enable |
| `result_formats` | keep only results whose URL ends in these extensions (e.g. `pdf,docx`) |
| `include_patterns` | keep only results whose URL contains any of these (comma list) |
| `exclude_patterns` | drop results whose URL contains any of these (comma list) |

```bash
# PDFs only
curl -s 'http://127.0.0.1:8888/search?q=distributed+systems&result_formats=pdf' | jq '.results[].url'

# By category
curl -s 'http://127.0.0.1:8888/search?q=react+hooks&categories=it&count=5' | jq '.results[].title'

# Specific engines, max 50 results
curl -s 'http://127.0.0.1:8888/search?q=rust&engines=duckduckgo,wikipedia_en&total_results=50' | jq '.results[].url'
```

## Configuration

`digse config` (or the `/config` web form) edits `~/.digse/config.toml`. It holds both search defaults and
serve settings.

```bash
digse config path                 # print the config file path
digse config show                 # print the resolved config as TOML
digse config init                 # write a default config to disk if none exists
digse config get serve.port       # read one value by dotted key
digse config set serve.port 9000  # set one value and persist it
```

Recognized keys:

- `search.concurrent_engines`, `search.timeout_seconds`, `search.count`,
  `search.total_results`, `search.show_engine_stats`, `search.language`,
  `search.result_type`, `search.categories`, `search.time_range`,
  `search.safe_search`
- `serve.host`, `serve.port`

Example config:

```toml
[search]
concurrent_engines = 12
timeout_seconds = 5
count = 10
total_results = 20
show_engine_stats = false
result_type = "web"

[serve]
host = "127.0.0.1"
port = 8888
```

Host/port changes take effect after `digse restart`. Explicit `/search`
parameters always override config defaults.

## Boot autostart

`digse startup` owns the boot entry only — separate from live `start`/`stop`:

- **Linux** — installs a systemd *user* service and enables `loginctl
  enable-linger`, so digse starts at boot, before login. No sudo: everything
  lives under `~/.config/systemd/user`.
- **Windows** — writes a per-user registry Run key
  (`HKCU\…\CurrentVersion\Run\digse`) that launches a hidden `.vbs` shim at
  logon. **No administrator/UAC prompt.**

```bash
digse startup status     # is autostart configured?
digse startup add        # enable for boot (does not start now — avoids a port clash)
digse startup remove     # disable + clean up
digse start              # ...then start it for this session
```

## Engines and categories

```bash
digse list engines           # list enabled engines
digse list engines --all     # include disabled engines
digse list categories        # list engine categories
```

## Output format

`GET /search` returns a single JSON object — a self-sufficient envelope that
echoes the request, reports pagination/timing, summarizes each engine's
outcome, and lists the results (each with a derived `domain`).

```json
{
  "digse": { "name": "digse", "version": "0.0.1", "generated_at": "2026-06-14T07:03:13Z" },
  "query": "what is rust",
  "result_type": "web",
  "request": { "query": "what is rust", "result_type": "web", "count": 10, "offset": 0, "timeout_seconds": 5, "language": null, "time_range": null, "safe_search": false },
  "pagination": { "returned": 10, "offset": 0, "limit": 10, "next_offset": 10, "has_more": true },
  "timing": { "duration_ms": 1842, "per_engine_timeout_ms": 5000 },
  "engines": {
    "queried": 90,
    "used": ["duckduckgo", "wikipedia_en", "..."],
    "summary": { "succeeded": 42, "partial": 3, "failed": 5, "timeout": 0, "rate_limited": 0 },
    "results_by_engine": { "duckduckgo": 10, "wikipedia_en": 8 },
    "stats": [ { "engine": "duckduckgo", "results_count": 10, "duration_ms": 312, "status": "success" } ]
  },
  "results": [
    {
      "title": "Rust Programming Language",
      "url": "https://www.rust-lang.org/",
      "domain": "rust-lang.org",
      "snippet": "A language empowering everyone to build reliable and efficient software.",
      "engine": "duckduckgo",
      "score": 1.0,
      "rank": 1
    }
  ],
  "count": 10,
  "total_results": 155
}
```

## Result types

`type` accepts: `web`, `images`, `videos`, `music`, `news`, `files`,
`torrents`, `academic`, `it`, `social`, `maps`, `shopping`, `weather`, `all`.

## Architecture

Digse is a Cargo workspace:

```
digse/
├── Cargo.toml           workspace configuration
├── .github/workflows/   release.yml — builds 5 platform binaries on `v*` tags
├── docs/                GitHub Pages site (index.html) + install scripts
└── crates/
    ├── core/            core types & traits (Engine, SearchQuery, SearchResponse)
    ├── engines/         238 engine implementations
    ├── http/            HTTP client with rate limiting & caching
    ├── parser/          HTML & JSON response parsing
    └── cli/             CLI binary + library (digse)
```

- **digse-core** — core abstractions, the `Engine` trait, query/result types
- **digse-engines** — 238 search engine implementations across 12 categories
- **digse-http** — HTTP client with rate limiting and caching
- **digse-parser** — HTML and JSON response parsing
- **digse** — the CLI application and reusable library

## Library usage

```toml
[dependencies]
digse = "0.0.1"
```

```rust
use digse::{DigseSearch, SearchBuilder, SearchQuery};
use digse_core::ResultType;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let search = SearchBuilder::new()
        .engines(vec!["duckduckgo".to_string()])
        .concurrent_engines(3)
        .build();

    let query = SearchQuery::new("rust programming")
        .with_result_type(ResultType::Web)
        .with_count(10);

    let response = search.search(&query).await?;
    println!("{}", serde_json::to_string_pretty(&response)?);

    Ok(())
}
```

## Development

```bash
cargo build                            # build the workspace
cargo test                             # run the test suite
cargo run -q -p digse -- start --port 8888
```

### Adding a new engine

1. Create a module in `crates/engines/src/`
2. Implement the `Engine` trait
3. Register the engine in `crates/engines/src/lib.rs`
4. Add tests

## Privacy

- No tracking or profiling
- The JSON `/search` endpoint needs no JavaScript; the web UI uses a little
- Self-hostable (`digse start`)
- Requests go directly from your machine to each engine

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

Licensed under MIT OR Apache-2.0.

## Disclaimer

This tool is for educational and research purposes. Please respect the terms of
service of the search engines you use and apply rate limiting appropriately.
