pub mod commands;

use std::sync::Mutex;

use commands::AppState;
use offering_infrastructure::{PasswordHasherService, SqliteBackupStorage, SqliteStore};
use tauri::Manager;

/// Starts the Tauri application.
///
/// # Panics
///
/// Panics (unwinding under the release profile, `panic = "unwind"`) if the
/// app-data directory, logging, database, or Tauri runtime cannot be
/// initialized. Startup failure is fatal by design: there is no safe degraded
/// mode without the database.
#[allow(clippy::too_many_lines)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data)?;
            // Operational log (plan Phase 8): the DB audit trail records
            // business events; this file records operational ones (database
            // error details, auth failures, restore outcomes). The UI shows
            // only generic messages for infrastructure failures.
            let file_appender =
                tracing_appender::rolling::daily(&app_data, "offering_recorder.log");
            let (file_writer, log_guard) = tracing_appender::non_blocking(file_appender);
            // Box::leak is intentional like the store below: the guard must
            // live for the process lifetime or logging stops.
            let _guard: &'static _ = Box::leak(Box::new(log_guard));
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                        "offering_recorder_lib=info,offering_application=info".into()
                    }),
                )
                .with_writer(file_writer)
                .init();
            let db_path = app_data.join("offering_recorder.sqlite");
            let db_str = db_path.to_string_lossy().to_string();
            let store = tauri::async_runtime::block_on(SqliteStore::open(&db_str))?;
            // Box::leak is intentional: tauri::State requires 'static, and for a
            // desktop app the one-time startup leak is acceptable. The store and
            // services live for the lifetime of the process.
            let store_ref: &'static SqliteStore = Box::leak(Box::new(store));
            let hasher: &'static PasswordHasherService = Box::leak(Box::new(PasswordHasherService));
            let backup_storage: &'static SqliteBackupStorage =
                Box::leak(Box::new(SqliteBackupStorage::new(store_ref.pool.clone())));
            let services = offering_application::services::Services::new(
                &store_ref.users,
                &store_ref.meetings,
                &store_ref.offerings,
                &store_ref.donors,
                &store_ref.donations,
                &store_ref.gift_aid,
                &store_ref.declaration_amendments,
                &store_ref.gasds_claims,
                &store_ref.community_buildings,
                &store_ref.bank_deposits,
                &store_ref.amendments,
                &store_ref.reports,
                &store_ref.backups,
                &store_ref.audit,
                hasher,
                backup_storage,
            );
            app.manage(AppState {
                services,
                session: Mutex::new(None),
                store: Mutex::new(store_ref),
                restoring: Mutex::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::needs_bootstrap,
            commands::bootstrap,
            commands::login,
            commands::logout,
            commands::current_user,
            commands::list_users,
            commands::create_user,
            commands::set_password,
            commands::set_user_active,
            commands::create_meeting,
            commands::list_meetings,
            commands::set_meeting_active,
            commands::update_meeting,
            commands::create_offering,
            commands::list_offerings,
            commands::list_all_offerings,
            commands::list_active_offerings,
            commands::list_locked_offerings,
            commands::get_offering,
            commands::offering_report,
            commands::set_attendance,
            commands::set_cash_denominations,
            commands::add_donation,
            commands::remove_donation,
            commands::validate_offering,
            commands::counter_sign,
            commands::checker_sign,
            commands::lock_offering,
            commands::withdraw_signature,
            commands::gasds_for_offering,
            commands::list_amendments,
            commands::create_amendment,
            commands::sign_amendment_counter,
            commands::sign_amendment_checker,
            commands::approve_amendment,
            commands::withdraw_amendment_signature,
            commands::delete_amendment,
            commands::list_donors,
            commands::list_active_donors,
            commands::get_donor,
            commands::search_donors,
            commands::create_donor,
            commands::update_donor,
            commands::set_donor_active,
            commands::redact_donor_pii,
            commands::create_gift_aid_declaration,
            commands::end_declaration,
            commands::revoke_declaration,
            commands::list_declarations,
            commands::list_donations_by_donor,
            commands::donor_eligibility_summary,
            commands::declaration_used_in_report,
            commands::create_declaration_amendment,
            commands::sign_declaration_amendment_counter,
            commands::sign_declaration_amendment_checker,
            commands::approve_declaration_amendment,
            commands::withdraw_declaration_amendment_signature,
            commands::delete_declaration_amendment,
            commands::list_declaration_amendments,
            commands::create_deposit,
            commands::record_deposit_actuals,
            commands::list_deposits,
            commands::delete_deposit,
            commands::available_offerings_for_deposit,
            commands::bookkeeping_report,
            commands::donor_report,
            commands::gift_aid_report,
            commands::gift_aid_readiness,
            commands::gasds_report,
            commands::gdpr_report,
            commands::deposit_report,
            commands::export_iif,
            commands::retention_report,
            commands::gasds_claim_readiness,
            commands::prepare_gasds_claim,
            commands::mark_gasds_claim_prepared,
            commands::submit_gasds_claim_for_review,
            commands::approve_gasds_claim,
            commands::reject_gasds_claim,
            commands::list_gasds_claims,
            commands::create_community_building,
            commands::list_community_buildings,
            commands::update_community_building,
            commands::set_meeting_event_details,
            commands::create_backup,
            commands::list_backups,
            commands::prune_expired_backups,
            commands::restore_backup,
            commands::list_audit_events,
            commands::recent_audit_events,
            commands::import_donors_csv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    /// Contract test (review O24/C3): every command name invoked from
    /// src/lib/api.ts must be registered in the `invoke_handler` list. This
    /// catches renamed or removed Rust commands that the hand-written
    /// bindings still reference - the class of drift behind review C3
    /// (argument drift still requires code review or generated bindings).
    #[test]
    fn all_api_ts_commands_are_registered() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let api_ts = std::fs::read_to_string(format!("{manifest}/../src/lib/api.ts"))
            .expect("src/lib/api.ts should exist");
        let lib_rs = include_str!("lib.rs");

        let mut registered = Vec::new();
        if let Some(start) = lib_rs.find("generate_handler![") {
            let body = &lib_rs[start + "generate_handler![".len()..];
            let body = &body[..body.find(']').unwrap_or(body.len())];
            for part in body.split(',') {
                let name = part.trim().trim_end_matches(',');
                if let Some(name) = name.strip_prefix("commands::") {
                    registered.push(name.to_string());
                }
            }
        }
        assert!(
            !registered.is_empty(),
            "failed to parse invoke_handler list"
        );

        // Extract quoted names following `run<...>(` invocations. Skip the
        // `run` DEFINITION itself (`function run<T>...`, whose body may
        // contain quoted words): only match when `run` is not part of a
        // larger identifier.
        let mut referenced = Vec::new();
        let bytes = api_ts.as_bytes();
        let mut i = 0;
        while let Some(pos) = api_ts[i..].find("run<") {
            let abs = i + pos;
            let prev = if abs == 0 { b' ' } else { bytes[abs - 1] };
            let is_def = abs >= 9 && &api_ts[abs - 9..abs] == "function ";
            if prev.is_ascii_alphanumeric() || prev == b'_' || is_def {
                i = abs + 4;
                continue;
            }
            let after = &api_ts[abs..];
            if let Some(open) = after.find('(') {
                let tail = &after[open..];
                if let Some(q1) = tail.find('"') {
                    let rest = &tail[q1 + 1..];
                    if let Some(q2) = rest.find('"') {
                        let name = &rest[..q2];
                        if name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                            referenced.push(name.to_string());
                        }
                    }
                }
            }
            i += pos + 4;
        }
        assert!(
            !referenced.is_empty(),
            "failed to extract commands from api.ts"
        );

        let missing: Vec<_> = referenced
            .iter()
            .filter(|c| !registered.contains(c))
            .collect();
        assert_eq!(
            missing.len(),
            0,
            "api.ts invokes unregistered commands: {missing:?}"
        );

        // Reverse direction (plan Phase 9): every registered command must be
        // reachable from api.ts, so dead commands cannot rot behind the IPC
        // boundary. Commands intentionally without UI bindings are allowlisted
        // with a reason.
        let allowlisted_without_ui: &[&str] = &[
            // Legacy/compat alias kept for old clients; new UI uses list_active_offerings.
            "list_all_offerings",
            // Bulk/power-user surface exercised via list_donors + search.
            "list_donors",
            "list_audit_events",
        ];
        let dead: Vec<_> = registered
            .iter()
            .filter(|c| !referenced.contains(c))
            .filter(|c| !allowlisted_without_ui.contains(&c.as_str()))
            .collect();
        assert_eq!(
            dead.len(),
            0,
            "registered commands without api.ts bindings: {dead:?}"
        );
        let _ = bytes;
    }

    /// Split a comma-separated list at depth zero, ignoring commas inside
    /// nested `{}`, `[]`, `()`, `<>` and inside string literals. Both the
    /// `api.ts` arg objects and the Rust signatures are parsed with this.
    ///
    /// `single_quote_is_string` is false for Rust sources: a `'` there is a
    /// lifetime (`State<'_, AppState>`) or lives inside a word
    /// (`offering's`), never a string delimiter for our purposes. It is true
    /// for TypeScript, which has no lifetimes.
    fn split_top_level(s: &str, single_quote_is_string: bool) -> Vec<String> {
        let mut parts = Vec::new();
        let mut depth_curly = 0i32;
        let mut depth_square = 0i32;
        let mut depth_paren = 0i32;
        let mut depth_angle = 0i32;
        let mut in_str: Option<char> = None;
        let mut escaped = false;
        let mut start = 0;
        let bytes: Vec<char> = s.chars().collect();
        let mut i = 0;
        let mut prev: Option<char> = None;
        while i < bytes.len() {
            let c = bytes[i];
            if let Some(q) = in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    in_str = None;
                }
            } else {
                match c {
                    '\'' if single_quote_is_string
                        && !prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '$') =>
                    {
                        in_str = Some(c);
                    }
                    // A `'` that does not open a string (Rust lifetime,
                    // word-internal apostrophe) falls through to `_`.
                    '"' | '`' => in_str = Some(c),
                    '{' => depth_curly += 1,
                    '}' => depth_curly -= 1,
                    '[' => depth_square += 1,
                    ']' => depth_square -= 1,
                    '(' => depth_paren += 1,
                    ')' => depth_paren -= 1,
                    '<' => depth_angle += 1,
                    '>' => depth_angle -= 1,
                    ',' if depth_curly == 0
                        && depth_square == 0
                        && depth_paren == 0
                        && depth_angle == 0 =>
                    {
                        parts.push(s[start..byte_index(s, i)].trim().to_string());
                        start = byte_index(s, i + 1);
                    }
                    _ => {}
                }
            }
            prev = Some(c);
            i += 1;
        }
        let tail = s[start..].trim().to_string();
        if !tail.is_empty() {
            parts.push(tail);
        }
        parts
    }

    /// Byte offset of the `i`-th char in `s`.
    fn byte_index(s: &str, i: usize) -> usize {
        s.char_indices().map(|(b, _)| b).nth(i).unwrap_or(s.len())
    }

    /// The `{...}` second argument of a `run("cmd", {...})` call, if the text
    /// after the command string is exactly `, {...}` (modulo whitespace).
    fn second_arg_object(after_cmd: &str) -> Option<&str> {
        let mut it = after_cmd.char_indices();
        for (_, c) in it.by_ref() {
            if c.is_whitespace() {
                continue;
            }
            if c == ',' {
                break;
            }
            return None;
        }
        for (b, c) in it {
            if c.is_whitespace() {
                continue;
            }
            if c == '{' {
                return Some(&after_cmd[b..]);
            }
            return None;
        }
        None
    }

    /// Extract `(command, arg-key)` pairs from `api.ts` `run("cmd", {...})`
    /// calls. Handles shorthand (`{ limit }`) and renamed (`{ day_of_week }`
    /// or `{ key: value }`) properties; nested values are ignored.
    fn extract_api_calls(api_ts: &str) -> Vec<(String, Vec<String>)> {
        let mut calls = Vec::new();
        let bytes = api_ts.as_bytes();
        let mut i = 0;
        while let Some(pos) = api_ts[i..].find("run<") {
            let abs = i + pos;
            let prev = if abs == 0 { b' ' } else { bytes[abs - 1] };
            let is_def = abs >= 9 && &api_ts[abs - 9..abs] == "function ";
            if prev.is_ascii_alphanumeric() || prev == b'_' || is_def {
                i = abs + 4;
                continue;
            }
            let after = &api_ts[abs..];
            let open = after.find('(').unwrap_or(usize::MAX);
            let tail = &after[open..];
            let q1 = tail.find('"');
            let (cmd, keys) = match q1 {
                Some(q1) => {
                    let rest = &tail[q1 + 1..];
                    match rest.find('"') {
                        Some(q2) => {
                            let cmd = rest[..q2].to_string();
                            let after_cmd = &rest[q2 + 1..];
                            // The args object must be the second call
                            // argument: `, {...}` with only whitespace
                            // around the comma. Otherwise the command takes
                            // no args and the next `{` in the file belongs to
                            // a different call.
                            let keys = match second_arg_object(after_cmd) {
                                Some(obj) => {
                                    match balanced_end(obj, '{', '}', true) {
                                        Some(e) => {
                                            let body = &obj[1..e];
                                            split_top_level(body, true)
                                                .iter()
                                                .filter_map(|part| {
                                                    let part = part.trim();
                                                    if part.is_empty() {
                                                        return None;
                                                    }
                                                    // `key: value` or shorthand `key`.
                                                    let key = match part.find(':') {
                                                        Some(c) => part[..c].trim(),
                                                        None => part
                                                            .split_whitespace()
                                                            .last()
                                                            .unwrap_or(part),
                                                    };
                                                    let key = key
                                                        .trim_matches(|c| c == '"' || c == '\'')
                                                        .to_string();
                                                    if key.is_empty() {
                                                        None
                                                    } else {
                                                        Some(key)
                                                    }
                                                })
                                                .collect()
                                        }
                                        None => Vec::new(),
                                    }
                                }
                                None => Vec::new(),
                            };
                            (cmd, keys)
                        }
                        None => (String::new(), Vec::new()),
                    }
                }
                None => (String::new(), Vec::new()),
            };
            if !cmd.is_empty() && cmd.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                calls.push((cmd, keys));
            }
            i = abs + 4;
        }
        calls
    }

    /// Byte index just past the balanced close delimiter, or `None`.
    /// Same `'` rule as [`split_top_level`]: Rust lifetimes and apostrophes
    /// must not open string literals. For TypeScript a `'` only opens a
    /// string when it does not continue a word (`offering's` in a comment
    /// must not swallow the rest of the file).
    fn balanced_end(
        s: &str,
        open: char,
        close: char,
        single_quote_is_string: bool,
    ) -> Option<usize> {
        let mut depth = 0i32;
        let mut in_str: Option<char> = None;
        let mut escaped = false;
        let mut prev: Option<char> = None;
        for (b, c) in s.char_indices() {
            if let Some(q) = in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    in_str = None;
                }
                prev = Some(c);
                continue;
            }
            if c == '"'
                || c == '`'
                || (c == '\''
                    && single_quote_is_string
                    && !prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '$'))
            {
                in_str = Some(c);
            } else if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(b);
                }
            }
            prev = Some(c);
        }
        None
    }

    /// Extract `command -> param names` from `src-tauri/src/commands/*.rs`.
    /// Skips the injected `state: State<..>` / `app: AppHandle` parameters;
    /// every other parameter must arrive under its exact Rust identifier
    /// (Tauri matches invoke args by exact key, with no case conversion).
    fn extract_rust_commands(commands_dir: &std::path::Path) -> HashMap<String, Vec<String>> {
        let mut map = HashMap::new();
        let entries = std::fs::read_dir(commands_dir).expect("commands dir should exist");
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .collect();
        files.sort();
        for path in files {
            if path.file_stem().is_some_and(|s| s == "mod") {
                continue;
            }
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("should read {}", path.display()));
            let mut i = 0;
            while i < src.len() {
                let rest = &src[i..];
                let Some((pos, prefix_len)) = ["pub async fn ", "pub fn "]
                    .iter()
                    .filter_map(|kw| rest.find(kw).map(|p| (p, kw.len())))
                    .min_by_key(|(p, _)| *p)
                else {
                    break;
                };
                let abs = i + pos + prefix_len;
                let after = &src[abs..];
                let paren = after.find('(').expect("fn must have params");
                let sig = &after[paren..];
                let end = balanced_end(sig, '(', ')', false).expect("balanced fn signature");
                let body = &sig[1..end];
                let name_end = after[..paren].trim_end().len();
                let name = after[..name_end]
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .to_string();
                let mut params = Vec::new();
                for part in split_top_level(body, false) {
                    let part = part.trim().to_string();
                    if part.is_empty() {
                        continue;
                    }
                    let pname = part.split(':').next().unwrap_or("").trim().to_string();
                    if pname == "state" || pname == "app" || pname.is_empty() {
                        continue;
                    }
                    params.push(pname);
                }
                if !name.is_empty() {
                    map.insert(name, params);
                }
                i = abs + paren + end + 1;
            }
        }
        map
    }

    /// Phase 0.3: every invoke arg key sent from `api.ts` must exactly match
    /// the corresponding `#[tauri::command]` parameter name. Tauri
    /// deserializes the args object by exact key (no `rename_all` exists on
    /// any command or input struct), so a camelCase key for a `snake_case`
    /// parameter fails at runtime with a missing-key error. This test parses
    /// both sides so the drift behind review C11 cannot regress.
    #[test]
    fn api_arg_keys_match_rust_params() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let api_ts = std::fs::read_to_string(format!("{manifest}/../src/lib/api.ts"))
            .expect("src/lib/api.ts should exist");
        let commands_dir = std::path::PathBuf::from(format!("{manifest}/src/commands"));

        let calls = extract_api_calls(&api_ts);
        assert!(!calls.is_empty(), "failed to extract api.ts invoke calls");
        let rust = extract_rust_commands(&commands_dir);
        assert!(!rust.is_empty(), "failed to extract Rust commands");

        let mut mismatches = Vec::new();
        for (cmd, keys) in &calls {
            match rust.get(cmd) {
                Some(params) => {
                    let want: HashSet<_> = params.iter().collect();
                    let got: HashSet<_> = keys.iter().collect();
                    if want != got {
                        mismatches.push(format!(
                            "{cmd}: api.ts sends {keys:?}, Rust expects {params:?}"
                        ));
                    }
                }
                None => mismatches.push(format!("{cmd}: no Rust command found")),
            }
        }
        assert_eq!(
            mismatches.len(),
            0,
            "IPC arg-key drift (api.ts vs commands/*.rs):\n{}",
            mismatches.join("\n")
        );
    }

    /// Field names of a flat Rust `pub struct NAME { ... }` in `src`.
    fn rust_struct_fields(src: &str, name: &str) -> Vec<String> {
        let marker = format!("pub struct {name} ");
        let start = src
            .find(&marker)
            .unwrap_or_else(|| panic!("struct {name} should exist"));
        let obj = &src[start + marker.len()..];
        let brace = obj.find('{').expect("struct body");
        let body = &obj[brace..];
        let end = balanced_end(body, '{', '}', false).expect("balanced struct");
        body[1..end]
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty()
                    || line.starts_with("///")
                    || line.starts_with("//")
                    || line.starts_with('#')
                {
                    return None;
                }
                // `pub field: Type,` — fields are one per line.
                let stripped = line.strip_prefix("pub ")?;
                Some(stripped.split(':').next().unwrap_or("").trim().to_string())
            })
            .filter(|f| !f.is_empty())
            .collect()
    }

    /// Field names of a flat TS `interface NAME { ... }` in `src`.
    /// Block (`/* ... */`) and line (`// ...`) comments are stripped first
    /// so doc text containing `:` or `;` cannot fake a field.
    fn ts_interface_fields(src: &str, name: &str) -> Vec<String> {
        let marker = format!("interface {name} ");
        let start = src
            .find(&marker)
            .unwrap_or_else(|| panic!("interface {name} should exist"));
        let obj = &src[start + marker.len()..];
        let brace = obj.find('{').expect("interface body");
        let body = &obj[brace..];
        let end = balanced_end(body, '{', '}', true).expect("balanced interface");
        let uncommented = strip_ts_comments(&body[1..end]);
        uncommented
            .split([';', '\n'])
            .filter_map(|field| {
                let field = field.trim().trim_end_matches(',').trim().to_string();
                if field.is_empty() || field.starts_with("//") || field.starts_with('*') {
                    return None;
                }
                let name = field
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches('?')
                    .trim()
                    .to_string();
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            })
            .collect()
    }

    /// Remove `/* ... */` and `// ...` comments from TypeScript source.
    /// String literals are honoured so `://` inside a string survives; a `'`
    /// only opens a string when it does not continue a word.
    fn strip_ts_comments(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let chars: Vec<char> = src.chars().collect();
        let mut i = 0;
        let mut in_str: Option<char> = None;
        let mut prev: Option<char> = None;
        while i < chars.len() {
            let c = chars[i];
            if let Some(q) = in_str {
                out.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 1;
                } else if c == q {
                    in_str = None;
                }
                prev = Some(c);
                i += 1;
                continue;
            }
            if c == '"'
                || c == '`'
                || (c == '\'' && !prev.is_some_and(|p| p.is_alphanumeric() || p == '_' || p == '$'))
            {
                in_str = Some(c);
                out.push(c);
                prev = Some(c);
                i += 1;
                continue;
            }
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            out.push(c);
            prev = Some(c);
            i += 1;
        }
        out
    }

    /// Variant names of a Rust enum (`AddDonation(..)` or
    /// `EndDeclaration { .. }` styles) in `src`.
    fn rust_enum_variants(src: &str, name: &str) -> Vec<(String, Vec<String>)> {
        let marker = format!("pub enum {name} ");
        let start = src
            .find(&marker)
            .unwrap_or_else(|| panic!("enum {name} should exist"));
        let obj = &src[start + marker.len()..];
        let brace = obj.find('{').expect("enum body");
        let body = &obj[brace..];
        let end = balanced_end(body, '{', '}', false).expect("balanced enum");
        let inner = &body[1..end];
        split_top_level(inner, false)
            .iter()
            .filter_map(|part| {
                let part = part.trim().to_string();
                if part.is_empty() || part.starts_with("///") || part.starts_with("//") {
                    return None;
                }
                // Skip attribute lines; variants are `Name(..)` / `Name {..}` / `Name`.
                let first_line = part.lines().last().unwrap_or("").trim().to_string();
                let first_line = first_line.trim_end_matches(',').trim().to_string();
                if first_line.is_empty() {
                    return None;
                }
                let (vname, fields) = if let Some(b) = first_line.find('{') {
                    let vname = first_line[..b].trim().to_string();
                    let fbody = &first_line[b..];
                    let e = balanced_end(fbody, '{', '}', false).unwrap_or(fbody.len());
                    let fields = fbody[1..e]
                        .split(',')
                        .filter_map(|f| {
                            let f = f.trim().to_string();
                            if f.is_empty() {
                                None
                            } else {
                                Some(f.split(':').next().unwrap_or("").trim().to_string())
                            }
                        })
                        .collect();
                    (vname, fields)
                } else if let Some(b) = first_line.find('(') {
                    (first_line[..b].trim().to_string(), Vec::new())
                } else {
                    (first_line, Vec::new())
                };
                if vname.is_empty() || !vname.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                {
                    None
                } else {
                    Some((vname, fields))
                }
            })
            .collect()
    }

    /// Phase 0.3: nested IPC payloads must match field-for-field. Serde has
    /// no `rename_all` on any input struct, and `Option` fields without
    /// `#[serde(default)]` must still be present on the wire, so a frontend
    /// object that merely omits an optional key fails to deserialize.
    #[test]
    fn nested_input_wire_shapes_match() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let root = std::path::PathBuf::from(format!("{manifest}/.."));
        let read = |p: &str| {
            std::fs::read_to_string(root.join(p)).unwrap_or_else(|_| panic!("should read {p}"))
        };
        let api_ts = read("src/lib/api.ts");
        let types_ts = read("src/lib/types.ts");
        let donors_rs = read("crates/application/src/donors.rs");
        let offerings_rs = read("crates/application/src/offerings.rs");
        let buildings_rs = read("crates/application/src/community_buildings.rs");
        let decl_amend_rs = read("crates/domain/src/entities/declaration_amendment.rs");
        let amend_rs = read("crates/domain/src/entities/amendment.rs");

        let mut mismatches = Vec::new();
        let check_fields =
            |label: &str, rust: Vec<String>, ts: Vec<String>, out: &mut Vec<String>| {
                let r: HashSet<_> = rust.iter().collect();
                let t: HashSet<_> = ts.iter().collect();
                if r != t {
                    out.push(format!("{label}: Rust {rust:?} vs TS {ts:?}"));
                }
            };

        // Struct inputs: field sets must be identical.
        check_fields(
            "CreateDonorInput",
            rust_struct_fields(&donors_rs, "CreateDonorInput"),
            ts_interface_fields(&api_ts, "CreateDonorInput"),
            &mut mismatches,
        );
        check_fields(
            "AddDonationInput",
            rust_struct_fields(&offerings_rs, "AddDonationInput"),
            ts_interface_fields(&types_ts, "AddDonationInput"),
            &mut mismatches,
        );
        check_fields(
            "CommunityBuildingInput",
            rust_struct_fields(&buildings_rs, "CommunityBuildingInput"),
            ts_interface_fields(&types_ts, "CommunityBuildingInput"),
            &mut mismatches,
        );

        // DeclarationChange: variant names plus inner field names. The TS
        // shape is `{ EndDeclaration: { at: string; reason: ... } }`.
        for (variant, fields) in rust_enum_variants(&decl_amend_rs, "DeclarationChange") {
            let marker = format!("{variant}:");
            if !types_ts.contains(&marker) && !types_ts.contains(&format!("{variant} ")) {
                mismatches.push(format!("DeclarationChange: TS missing variant {variant}"));
                continue;
            }
            for f in fields {
                // Inner payload object must mention the field name.
                let variant_pos = types_ts.find(&variant).unwrap_or(usize::MAX);
                let window = &types_ts[variant_pos..(variant_pos + 200).min(types_ts.len())];
                if !window.contains(&f) {
                    mismatches.push(format!(
                        "DeclarationChange::{variant}: TS payload missing field {f}"
                    ));
                }
            }
        }

        // AmendmentChange: variant names must exist on both sides.
        let rust_variants: HashSet<String> = rust_enum_variants(&amend_rs, "AmendmentChange")
            .into_iter()
            .map(|(v, _)| v)
            .collect();
        for v in &rust_variants {
            if !types_ts.contains(v) {
                mismatches.push(format!("AmendmentChange: TS missing variant {v}"));
            }
        }
        // TS uses externally-tagged `{ Variant: ... }` objects; every TS
        // variant key must be a real Rust variant.
        for marker in [
            "AddDonation",
            "RemoveDonation",
            "SetCashDenominations",
            "SetAttendance",
            "SetDescription",
        ] {
            if types_ts.contains(marker) && !rust_variants.contains(marker) {
                mismatches.push(format!("AmendmentChange: Rust missing variant {marker}"));
            }
        }

        assert_eq!(
            mismatches.len(),
            0,
            "nested IPC shape drift:\n{}",
            mismatches.join("\n")
        );
    }

    /// Phase 0.3: the exact JSON the frontend sends must deserialize into
    /// the real Rust input types. This is the executable half of the wire
    /// contract (the parsers above are the drift-detection half).
    #[test]
    fn frontend_shaped_inputs_deserialize() {
        // AddDonationInput as DonationForm.svelte builds it (snake_case keys,
        // explicit nulls, `date` present).
        let donation: offering_application::offerings::AddDonationInput =
            serde_json::from_value(serde_json::json!({
                "donor_id": null,
                "amount_pence": 500,
                "donation_type": "General",
                "payment_method": "Cash",
                "classification": "LooseCash",
                "reference": null,
                "cash_denominations": { "counts": { "Coin100p": 5 } },
                "date": null
            }))
            .expect("frontend AddDonationInput shape must deserialize");
        assert_eq!(donation.amount_pence, 500);

        // CreateDonorInput as Donors.svelte builds it.
        let donor: offering_application::donors::CreateDonorInput =
            serde_json::from_value(serde_json::json!({
                "first_name": "Ada",
                "last_name": "Lovelace",
                "preferred_name": null,
                "donor_type": "Uk",
                "address_line1": null,
                "address_line2": null,
                "address_line3": null,
                "postcode": null,
                "email": null,
                "phone": null,
                "lawful_basis": "Legitimate interest",
                "privacy_notice_version": null
            }))
            .expect("frontend CreateDonorInput shape must deserialize");
        assert_eq!(donor.first_name, "Ada");

        // DeclarationChange as DonorDetail.svelte builds it.
        let change: offering_domain::entities::declaration_amendment::DeclarationChange =
            serde_json::from_value(serde_json::json!({
                "EndDeclaration": { "at": "2026-04-06T00:00:00Z", "reason": "moved" }
            }))
            .expect("frontend DeclarationChange shape must deserialize");
        assert!(matches!(
            change,
            offering_domain::entities::declaration_amendment::DeclarationChange::EndDeclaration { .. }
        ));
    }
}
