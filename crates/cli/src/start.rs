//! HTTP server for `digse start`.
//!
//! The daemon child binds the listener, writes a PID file, then serves forever:
//!
//!   GET /                -> HTML search page (textbox + button; JS hits /search)
//!   GET /search?q=...    -> JSON search envelope (200)
//!   GET /health          -> `{"status":"ok"}` (200)
//!   anything else        -> 404 JSON
//!
//! No external HTTP framework: `tokio` (already a dependency) is enough for a
//! request-line parser and a handful of GET routes, which keeps digse light.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use digse::SearchResponse;
use digse_core::{ResultType, SearchQuery, TimeRange};

use crate::pidfile;

/// All `ResultType` variants (lowercase), as accepted by `/search?type=`.
const RESULT_TYPES: [&str; 14] = [
    "web", "images", "videos", "music", "news", "files", "torrents", "academic", "it", "social",
    "maps", "shopping", "weather", "all",
];

/// All `TimeRange` variants.
const TIME_RANGES: [&str; 4] = ["day", "week", "month", "year"];

/// All `EngineCategory` variants (lowercase), used for category checkboxes.
const CATEGORY_NAMES: [&str; 12] = [
    "general", "images", "videos", "music", "news", "science", "it", "files", "social", "maps",
    "shopping", "weather",
];

/// What a routed request produces: a rendered body, or a redirect.
enum Reply {
    Body {
        status: u16,
        content_type: &'static str,
        body: String,
    },
    Redirect {
        location: String,
    },
}

impl Reply {
    fn body(status: u16, content_type: &'static str, body: String) -> Self {
        Reply::Body {
            status,
            content_type,
            body,
        }
    }
}

/// Load the persisted config, falling back to defaults on any error.
fn load_cfg() -> digse::DigseConfig {
    digse::DigseConfig::load().unwrap_or_default()
}

/// HTML-escape a string for safe interpolation into a page.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Bind the listener, record the PID file, then run the accept loop. This is the
/// foreground entry point used by the hidden `__start_foreground__` subcommand.
pub async fn run_foreground(host: &str, port: u16) -> std::io::Result<()> {
    let host = if host.is_empty() { "127.0.0.1" } else { host };
    let listener = bind_and_record(host, port).await?;
    accept_loop(listener).await
}

/// Bind the socket and — only after a successful bind — write the PID file, so
/// "PID file exists" is equivalent to "port is held". A bind failure propagates
/// and the daemon child exits without writing anything.
async fn bind_and_record(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let addr: SocketAddr = format!("{}:{}", host, port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let listener = bind_with_retry(addr).await?;
    eprintln!("digse start: listening on http://{}", addr);

    let record = pidfile::PidRecord {
        pid: std::process::id(),
        host: host.to_string(),
        port,
    };
    pidfile::write(&record)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(listener)
}

/// Bind the listener, retrying for a few seconds on `AddrInUse`. During
/// `digse restart` the just-stopped predecessor can hold the port briefly after
/// the parent observes it gone; retrying makes the hand-off seamless.
async fn bind_with_retry(addr: SocketAddr) -> std::io::Result<TcpListener> {
    const ATTEMPTS: u32 = 25;
    const INTERVAL: Duration = Duration::from_millis(200);

    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..ATTEMPTS {
        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok(listener),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                last_err = Some(e);
                tokio::time::sleep(INTERVAL).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrInUse, "bind failed")))
}

async fn accept_loop(listener: TcpListener) -> std::io::Result<()> {
    loop {
        let (stream, _peer) = listener.accept().await?;
        tokio::spawn(handle_connection(stream));
    }
}

async fn handle_connection(stream: TcpStream) {
    if let Err(e) = handle(stream).await {
        eprintln!("serve: connection error: {}", e);
    }
}

/// Handle a single HTTP/1.x connection (request line + headers, no body).
async fn handle(
    stream: TcpStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Request line: "GET /search?q=... HTTP/1.1"
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await? == 0 {
        return Ok(()); // empty connection
    }
    // Drain headers up to the blank line, capturing Content-Length for POST bodies.
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).await? == 0 {
            break;
        }
        if header.trim().is_empty() {
            break;
        }
        if let Some((name, val)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = val.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");

    let (route, query) = target
        .split_once('?')
        .map(|(r, q)| (r, q))
        .unwrap_or((target, ""));

    // For POST requests advertising a body, read exactly Content-Length bytes.
    // (The server always replies with Connection: close, so nothing follows.)
    let req_body = if method == "POST" && content_length > 0 {
        let mut buf = vec![0u8; content_length];
        reader.read_exact(&mut buf).await?;
        String::from_utf8_lossy(&buf).into_owned()
    } else {
        String::new()
    };

    let reply = match (method, route) {
        ("GET", "/") => Reply::body(200, "text/html", root_html_body(&load_cfg())),
        ("GET", "/search") => Reply::body(200, "application/json", search_handler(query).await),
        ("GET", "/health") => {
            Reply::body(200, "application/json", r#"{"status":"ok"}"#.to_string())
        }
        ("GET", "/config") => Reply::body(200, "text/html", config_form_html(&load_cfg(), None)),
        ("POST", "/config") => config_post_handler(&req_body),
        ("GET", other) => Reply::body(
            404,
            "application/json",
            format!(r#"{{"error":"not found","path":"{}"}}"#, other),
        ),
        (method, _) => Reply::body(
            405,
            "application/json",
            format!(r#"{{"error":"method not allowed","method":"{}"}}"#, method),
        ),
    };

    match reply {
        Reply::Body {
            status,
            content_type,
            body,
        } => write_response(&mut write_half, status, content_type, &body).await,
        Reply::Redirect { location } => write_redirect(&mut write_half, &location).await,
    }
}

/// Execute a search from query-string parameters and return the JSON envelope.
async fn search_handler(query: &str) -> String {
    let params = parse_query(query);
    let (response, request) = match build_and_run(&params).await {
        Ok(pair) => pair,
        Err(e) => {
            let msg = e.to_string().replace('\\', "\\\\").replace('"', "\\\"");
            return format!(
                r#"{{"digse":{{"name":"digse","version":"{}"}},"error":"{}"}}"#,
                digse::VERSION, msg
            );
        }
    };
    let envelope = digse::build_response_envelope(&response, &request, digse::VERSION);
    digse::render_json(&envelope, true)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}

/// Parse a URL-encoded query string into a key→value map.
fn parse_query(query: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        let key = decode(k);
        let val = decode(v);
        if !key.is_empty() {
            map.insert(key, val);
        }
    }
    map
}

/// Percent-decode a query token (form-encoded: `+` → space, `%XX` → byte).
fn decode(s: &str) -> String {
    let plus_to_space = s.replace('+', " ");
    urlencoding::decode(&plus_to_space)
        .map(|cow| cow.into_owned())
        .unwrap_or_else(|_| plus_to_space)
}

/// Build a `SearchQuery` + config from the request params, run the search, apply
/// the migrated URL post-filters and `total_results` ceiling, and return the
/// response together with the request that produced it (so the envelope can echo
/// it). Defaults for `count` / `total_results` / concurrency / timeout come from
/// the persisted config, with explicit query params overriding them.
async fn build_and_run(
    params: &HashMap<String, String>,
) -> Result<(SearchResponse, SearchQuery), Box<dyn std::error::Error + Send + Sync>> {
    let q = params
        .get("q")
        .or_else(|| params.get("query"))
        .cloned()
        .unwrap_or_default();
    if q.trim().is_empty() {
        return Err("missing required parameter 'q' (the search query)".into());
    }

    let cfg = digse::DigseConfig::load().unwrap_or_default();

    let result_type = params
        .get("type")
        .and_then(|t| ResultType::from_str(t))
        .or_else(|| cfg.search.result_type.as_deref().and_then(ResultType::from_str))
        .unwrap_or(ResultType::Web);

    let mut search_query = SearchQuery::new(&q)
        .with_result_type(result_type)
        .with_count(parse_usize(params, "count", cfg.search.count))
        .with_offset(parse_usize(params, "offset", 0))
        .with_timeout(parse_u64(params, "timeout", cfg.search.timeout_seconds));

    if let Some(lang) = params.get("language") {
        search_query = search_query.with_language(lang);
    }
    let time_range_src = params
        .get("time_range")
        .cloned()
        .or_else(|| cfg.search.time_range.clone());
    if let Some(range_str) = time_range_src {
        if let Some(range) = TimeRange::from_str(&range_str) {
            search_query = search_query.with_time_range(range);
        }
    }
    let safe_search = match params.get("safe_search") {
        Some(v) => matches!(v.as_str(), "1" | "true" | "yes" | "on"),
        None => cfg.search.safe_search,
    };
    search_query = search_query.with_safe_search(safe_search);

    let config = digse::SearchConfig {
        engine_selection: build_engine_selection(params, cfg.search.categories.as_deref()),
        concurrent_engines: parse_usize(params, "concurrent_engines", cfg.search.concurrent_engines),
        timeout_seconds: parse_u64(params, "timeout", cfg.search.timeout_seconds),
        show_engine_stats: true,
    };

    let search = digse::DigseSearch::with_config(config);
    let mut response = search.search(&search_query).await?;

    // Migrated post-filters + total_results ceiling (from the removed `search`
    // command's --result-formats / --include-patterns / --exclude-patterns /
    // --total-results knobs).
    let filters = crate::filters::PostFilters::from_raw(
        params.get("result_formats").map(String::as_str),
        params.get("include_patterns").map(String::as_str),
        params.get("exclude_patterns").map(String::as_str),
    );
    crate::filters::apply_post_filters(&mut response, &filters);

    let total_results = parse_usize(params, "total_results", cfg.search.total_results);
    if response.results.len() > total_results {
        response.results.truncate(total_results);
        response.total_results = response.results.len();
    }

    Ok((response, search_query))
}

fn build_engine_selection(
    params: &HashMap<String, String>,
    cfg_categories: Option<&str>,
) -> digse::EngineSelection {
    if let Some(engines) = params.get("engines") {
        let list: Vec<String> = engines.split(',').map(|s| s.trim().to_lowercase()).collect();
        digse::EngineSelection::Specific(list)
    } else if let Some(exclude) = params.get("exclude_engines") {
        let list: Vec<String> = exclude.split(',').map(|s| s.trim().to_lowercase()).collect();
        digse::EngineSelection::Exclude(list)
    } else {
        // Fall back to the request `categories` param, then the persisted default.
        let src = params.get("categories").map(String::as_str).or(cfg_categories);
        match src {
            Some(categories) if !categories.trim().is_empty() => {
                let list: Vec<digse_core::EngineCategory> = categories
                    .split(',')
                    .filter_map(|s| digse_core::EngineCategory::from_str(s.trim()))
                    .collect();
                digse::EngineSelection::Categories(list)
            }
            _ => digse::EngineSelection::All,
        }
    }
}

fn parse_usize(params: &HashMap<String, String>, key: &str, default: usize) -> usize {
    params.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn parse_u64(params: &HashMap<String, String>, key: &str, default: u64) -> u64 {
    params.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// The HTML homepage: a textbox + Search button. The submit handler fetches
/// `/search?q=...` and renders the JSON `results[]` inline (rather than
/// navigating the browser to the raw JSON).
fn root_html_body(cfg: &digse::DigseConfig) -> String {
    // result-type select options (mark the configured default selected)
    let rt = cfg.search.result_type.clone().unwrap_or_default();
    let rt_opts = std::iter::once("")
        .chain(RESULT_TYPES.iter().copied())
        .map(|o| {
            let sel = if o == rt.as_str() { " selected" } else { "" };
            let label = if o.is_empty() { "web (default)" } else { o };
            format!(
                r#"<option value="{}"{sel}>{}</option>"#,
                esc(o),
                esc(label),
                sel = sel
            )
        })
        .collect::<String>();

    // time-range select options
    let tr = cfg.search.time_range.clone().unwrap_or_default();
    let tr_opts = std::iter::once("")
        .chain(TIME_RANGES.iter().copied())
        .map(|o| {
            let sel = if o == tr.as_str() { " selected" } else { "" };
            let label = if o.is_empty() { "any" } else { o };
            format!(
                r#"<option value="{}"{sel}>{}</option>"#,
                esc(o),
                esc(label),
                sel = sel
            )
        })
        .collect::<String>();

    // category checkboxes (mark the configured defaults checked; empty = all)
    let cats_val = cfg.search.categories.clone().unwrap_or_default();
    let cats: Vec<&str> = cats_val
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let cat_boxes = CATEGORY_NAMES
        .iter()
        .map(|c| {
            let ck = if cats.contains(c) { " checked" } else { "" };
            format!(
                r#"<label class="chk"><input type="checkbox" name="cat" value="{c}"{ck}> {c}</label>"#,
                c = c,
                ck = ck
            )
        })
        .collect::<String>();

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>digse</title>
<style>
  body {{ font: 15px system-ui, -apple-system, sans-serif; max-width: 760px; margin: 2rem auto; padding: 0 1rem; color: #222; }}
  header h1 {{ font-size: 1.6rem; margin: 0 0 1rem; }}
  header .settings {{ float: right; font-size: .8rem; }}
  form#f {{ display: flex; gap: .5rem; margin-bottom: .5rem; }}
  input#q {{ flex: 1; padding: 8px 10px; font-size: 15px; border: 1px solid #ccc; border-radius: 4px; }}
  button {{ padding: 8px 18px; font-size: 15px; border: 0; border-radius: 4px; background: #1a73e8; color: #fff; cursor: pointer; }}
  .opts {{ display: flex; flex-wrap: wrap; gap: .4rem .9rem; align-items: center; margin: .5rem 0 0; font-size: .9rem; }}
  .opts label {{ display: flex; align-items: center; gap: .3rem; color: #444; }}
  .opts select, .opts input {{ padding: 4px 6px; font-size: .9rem; border: 1px solid #ccc; border-radius: 4px; }}
  .chks {{ display: flex; flex-wrap: wrap; gap: .25rem .9rem; margin: .35rem 0 .25rem; font-size: .85rem; color: #444; }}
  .chk {{ white-space: nowrap; }}
  details {{ margin: .75rem 0; border-top: 1px solid #eee; padding-top: .5rem; }}
  details summary {{ cursor: pointer; color: #555; font-size: .9rem; }}
  .grid {{ display: grid; grid-template-columns: 11rem 1fr; gap: .4rem .6rem; margin: .5rem 0; align-items: center; }}
  .grid label {{ color: #666; font-size: .85rem; }}
  .grid input {{ padding: 5px 7px; font-size: .9rem; border: 1px solid #ccc; border-radius: 4px; }}
  #status {{ color: #666; font-size: .85rem; margin: .5rem 0; }}
  .item {{ margin: 1rem 0; }}
  .item a {{ color: #1a0dab; text-decoration: none; font-size: 1.08rem; }}
  .item a:hover {{ text-decoration: underline; }}
  .item .meta {{ color: #006621; font-size: .8rem; margin: 2px 0; }}
  .item .snippet {{ color: #444; font-size: .92rem; }}
  footer {{ margin-top: 2rem; color: #999; font-size: .75rem; }}
</style>
</head>
<body>
<header>
  <span class="settings"><a href="/config">Settings</a></span>
  <h1>digse</h1>
</header>
<form id="f">
  <input type="text" id="q" placeholder="Search the engines..." autofocus>
  <button type="submit">Search</button>
</form>
<div class="opts">
  <label>Type <select id="rt">{rt_opts}</select></label>
  <label>Time <select id="tr">{tr_opts}</select></label>
  <label>Lang <input type="text" id="lang" value="{language}" size="6" placeholder="en-US"></label>
  <label>Count <input type="number" id="count" value="{count}" size="3" min="1"></label>
  <label><input type="checkbox" id="ss"{safe_search}> Safe search</label>
</div>
<div class="chks">{cat_boxes}</div>
<details>
  <summary>Advanced</summary>
  <div class="grid">
    <label>Engines</label><input type="text" id="engines" placeholder="comma list, e.g. google,bing">
    <label>Exclude engines</label><input type="text" id="exclude_engines" placeholder="comma list">
    <label>Concurrent engines</label><input type="number" id="concurrent_engines" value="{concurrent_engines}" min="1">
    <label>Timeout (s)</label><input type="number" id="timeout" value="{timeout_seconds}" min="1">
    <label>Max total results</label><input type="number" id="total_results" value="{total_results}" min="1">
    <label>Result formats</label><input type="text" id="result_formats" placeholder="pdf,docx">
    <label>Include patterns</label><input type="text" id="include_patterns" placeholder="URL substrings">
    <label>Exclude patterns</label><input type="text" id="exclude_patterns" placeholder="URL substrings">
  </div>
</details>
<div id="status"></div>
<div id="results"></div>
<footer>digse {ver} &middot; results from <code>/search</code></footer>
<script>
const f = document.getElementById('f');
const q = document.getElementById('q');
const box = document.getElementById('results');
const status = document.getElementById('status');
const v = id => {{ const el = document.getElementById(id); return el ? el.value.trim() : ''; }};
const ck = id => {{ const el = document.getElementById(id); return !!(el && el.checked); }};

f.addEventListener('submit', async (e) => {{
  e.preventDefault();
  const term = q.value.trim();
  if (!term) return;
  const p = new URLSearchParams();
  p.set('q', term);
  if (v('rt')) p.set('type', v('rt'));
  const cats = Array.from(document.querySelectorAll('input[name="cat"]:checked')).map(c => c.value);
  if (cats.length) p.set('categories', cats.join(','));
  if (v('tr')) p.set('time_range', v('tr'));
  if (ck('ss')) p.set('safe_search', '1');
  if (v('lang')) p.set('language', v('lang'));
  if (v('count')) p.set('count', v('count'));
  if (v('engines')) p.set('engines', v('engines'));
  if (v('exclude_engines')) p.set('exclude_engines', v('exclude_engines'));
  if (v('concurrent_engines')) p.set('concurrent_engines', v('concurrent_engines'));
  if (v('timeout')) p.set('timeout', v('timeout'));
  if (v('total_results')) p.set('total_results', v('total_results'));
  if (v('result_formats')) p.set('result_formats', v('result_formats'));
  if (v('include_patterns')) p.set('include_patterns', v('include_patterns'));
  if (v('exclude_patterns')) p.set('exclude_patterns', v('exclude_patterns'));

  status.textContent = 'searching...';
  box.innerHTML = '';
  try {{
    const res = await fetch('/search?' + p.toString());
    const data = await res.json();
    if (data.error) {{ status.textContent = 'Error: ' + data.error; return; }}
    const list = data.results || [];
    const total = (typeof data.total_results === 'number') ? data.total_results : null;
    status.textContent = list.length + ' result' + (list.length === 1 ? '' : 's')
      + (total !== null ? ' (total_results=' + total + ')' : '');
    if (!list.length) {{ status.textContent = 'No results.'; return; }}
    for (const it of list) {{
      const div = document.createElement('div');
      div.className = 'item';
      const a = document.createElement('a');
      a.href = it.url; a.target = '_blank'; a.rel = 'noopener';
      a.textContent = it.title || it.url;
      div.appendChild(a);
      const meta = document.createElement('div');
      meta.className = 'meta';
      meta.textContent = [it.domain, it.engine].filter(Boolean).join(' · ');
      div.appendChild(meta);
      if (it.snippet) {{
        const sn = document.createElement('div');
        sn.className = 'snippet';
        sn.textContent = it.snippet;
        div.appendChild(sn);
      }}
      box.appendChild(div);
    }}
  }} catch (err) {{
    status.textContent = 'Request failed: ' + err;
  }}
}});
</script>
</body>
</html>"##,
        rt_opts = rt_opts,
        tr_opts = tr_opts,
        language = esc(cfg.search.language.as_deref().unwrap_or("")),
        count = cfg.search.count,
        concurrent_engines = cfg.search.concurrent_engines,
        timeout_seconds = cfg.search.timeout_seconds,
        total_results = cfg.search.total_results,
        safe_search = if cfg.search.safe_search { " checked" } else { "" },
        cat_boxes = cat_boxes,
        ver = digse::VERSION,
    )
}

/// Write an HTTP/1.1 response with the given status, content type, and body.
async fn write_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    status: u16,
    content_type: &str,
    body: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}; charset=utf-8\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n",
        len = body.len(),
    );
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

/// Write an HTTP/1.1 303 See Other redirect with an empty body. Used for the
/// POST-Redirect-GET pattern after `/config` saves, so the browser re-GETs the
/// form instead of re-submitting on refresh.
async fn write_redirect(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    location: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let header = format!(
        "HTTP/1.1 303 See Other\r\n\
         Location: {location}\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: *\r\n\
         \r\n",
        location = location,
    );
    writer.write_all(header.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

/// Handle a POSTed `/config` form: apply each field through `DigseConfig::set`,
/// re-rendering the form (with the error) on any validation failure, otherwise
/// persist and redirect to GET `/config`.
fn config_post_handler(body: &str) -> Reply {
    let mut cfg = load_cfg();
    let form = parse_query(body);

    // Render the form with an error message, preserving partial edits.
    macro_rules! bail {
        ($cfg:expr, $msg:expr) => {
            return Reply::body(200, "text/html", config_form_html($cfg, Some(&$msg.to_string())))
        };
    }

    // Single-valued string/number keys — applied only when the form sent them.
    for key in [
        "search.concurrent_engines",
        "search.timeout_seconds",
        "search.count",
        "search.total_results",
        "search.language",
        "search.result_type",
        "search.time_range",
        "serve.host",
        "serve.port",
    ] {
        if let Some(v) = form.get(key) {
            if let Err(e) = cfg.set(key, v) {
                bail!(&cfg, e);
            }
        }
    }

    // `search.categories` is a checkbox group (one `cat_<name>` per category);
    // assemble the comma list so unchecking every box clears the default.
    let cats: Vec<&str> = CATEGORY_NAMES
        .iter()
        .copied()
        .filter(|c| form.contains_key(&format!("cat_{}", c)))
        .collect();
    if let Err(e) = cfg.set("search.categories", &cats.join(",")) {
        bail!(&cfg, e);
    }

    // Boolean checkboxes: present -> "on" -> true; absent -> "false" -> false.
    for key in ["search.show_engine_stats", "search.safe_search"] {
        let v = if form.contains_key(key) { "on" } else { "false" };
        if let Err(e) = cfg.set(key, v) {
            bail!(&cfg, e);
        }
    }

    match cfg.save() {
        Ok(_) => Reply::Redirect {
            location: "/config".to_string(),
        },
        Err(e) => bail!(&cfg, e),
    }
}

/// Render the `/config` settings form, pre-filled from the persisted config.
/// Covers every settable key so the web UI matches the `digse config` CLI.
fn config_form_html(cfg: &digse::DigseConfig, message: Option<&String>) -> String {
    let val = |k: &str| cfg.get(k).unwrap_or_default();
    let is_true = |k: &str| val(k) == "true";

    let rt = val("search.result_type");
    let rt_opts = std::iter::once("")
        .chain(RESULT_TYPES.iter().copied())
        .map(|o| {
            let sel = if o == rt.as_str() { " selected" } else { "" };
            let label = if o.is_empty() { "(default — web)" } else { o };
            format!(
                r#"<option value="{}"{sel}>{}</option>"#,
                esc(o),
                esc(label),
                sel = sel
            )
        })
        .collect::<String>();

    let tr = val("search.time_range");
    let tr_opts = std::iter::once("")
        .chain(TIME_RANGES.iter().copied())
        .map(|o| {
            let sel = if o == tr.as_str() { " selected" } else { "" };
            let label = if o.is_empty() { "(any)" } else { o };
            format!(
                r#"<option value="{}"{sel}>{}</option>"#,
                esc(o),
                esc(label),
                sel = sel
            )
        })
        .collect::<String>();

    let cats_val = val("search.categories");
    let cats: Vec<&str> = cats_val
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let cat_boxes = CATEGORY_NAMES
        .iter()
        .map(|c| {
            let ck = if cats.contains(c) { " checked" } else { "" };
            format!(
                r#"<label class="chk"><input type="checkbox" name="cat_{c}"{ck}> {c}</label>"#,
                c = c,
                ck = ck
            )
        })
        .collect::<String>();

    let banner = match message {
        Some(m) => format!(r#"<div class="msg">{}</div>"#, esc(m)),
        None => String::new(),
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>digse · settings</title>
<style>
  body {{ font: 15px system-ui, -apple-system, sans-serif; max-width: 720px; margin: 2rem auto; padding: 0 1rem; color: #222; }}
  h1 {{ font-size: 1.4rem; margin: 0 0 .5rem; }}
  h2 {{ font-size: 1.05rem; margin: 1.5rem 0 .5rem; border-bottom: 1px solid #eee; padding-bottom: .25rem; }}
  .row {{ margin: .5rem 0; display: flex; align-items: center; gap: .5rem; }}
  .row > label {{ width: 13rem; color: #444; }}
  .row input[type=text], .row input[type=number], select {{ flex: 1; padding: 6px 8px; font-size: 14px; border: 1px solid #ccc; border-radius: 4px; }}
  .chks {{ display: flex; flex-wrap: wrap; gap: .25rem .9rem; margin: .25rem 0 0 13rem; font-size: .88rem; }}
  .chks .lead {{ width: 100%; margin-left: -13rem; color: #444; }}
  .note {{ color: #999; font-size: .78rem; margin: .25rem 0 0 13rem; }}
  .msg {{ background: #fdecea; border: 1px solid #f5c6cb; color: #a94442; padding: .5rem .75rem; border-radius: 4px; margin: .75rem 0; }}
  button {{ margin-top: 1rem; padding: 8px 18px; font-size: 15px; border: 0; border-radius: 4px; background: #1a73e8; color: #fff; cursor: pointer; }}
  a.back {{ display: inline-block; color: #1a73e8; }}
  code {{ background: #f4f4f4; padding: 0 .25rem; border-radius: 3px; }}
</style>
</head>
<body>
<header><h1>digse settings</h1></header>
<a class="back" href="/">&larr; back to search</a>
{banner}
<form method="POST" action="/config">
  <h2>Search defaults</h2>
  <div class="row"><label>Result type</label><select name="search.result_type">{rt_opts}</select></div>
  <span class="chks lead">Categories (default selection)</span>
  <div class="chks">{cat_boxes}</div>
  <div class="row"><label>Time range</label><select name="search.time_range">{tr_opts}</select></div>
  <div class="row"><label>Language</label><input type="text" name="search.language" value="{language}" placeholder="e.g. en-US"></div>
  <div class="row"><label>Safe search</label><input type="checkbox" name="search.safe_search"{safe_search}></div>
  <div class="row"><label>Results per engine</label><input type="number" name="search.count" value="{count}" min="1"></div>
  <div class="row"><label>Max total results</label><input type="number" name="search.total_results" value="{total_results}" min="1"></div>
  <div class="row"><label>Concurrent engines</label><input type="number" name="search.concurrent_engines" value="{concurrent_engines}" min="1"></div>
  <div class="row"><label>Per-engine timeout (s)</label><input type="number" name="search.timeout_seconds" value="{timeout_seconds}" min="1"></div>
  <div class="row"><label>Show engine stats</label><input type="checkbox" name="search.show_engine_stats"{show_stats}></div>

  <h2>Server</h2>
  <div class="row"><label>Bind host</label><input type="text" name="serve.host" value="{host}"></div>
  <div class="row"><label>Bind port</label><input type="number" name="serve.port" value="{port}" min="1" max="65535"></div>
  <div class="note">Host/port changes take effect after <code>digse restart</code>.</div>

  <button type="submit">Save</button>
</form>
</body>
</html>"##,
        banner = banner,
        rt_opts = rt_opts,
        cat_boxes = cat_boxes,
        tr_opts = tr_opts,
        language = esc(&val("search.language")),
        safe_search = if is_true("search.safe_search") { " checked" } else { "" },
        count = esc(&val("search.count")),
        total_results = esc(&val("search.total_results")),
        concurrent_engines = esc(&val("search.concurrent_engines")),
        timeout_seconds = esc(&val("search.timeout_seconds")),
        show_stats = if is_true("search.show_engine_stats") { " checked" } else { "" },
        host = esc(&val("serve.host")),
        port = esc(&val("serve.port")),
    )
}
