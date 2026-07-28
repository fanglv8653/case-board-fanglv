//! MCP server secure credential projection.
//!
//! This module deliberately does not read or write `settings.json`. Callers
//! supply a persistence closure so vault changes and the future settings
//! projection can be committed as one recoverable operation.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::credentials::{CredentialBackend, CredentialError, CredentialLocator, SecretValue};

use super::mcp_bridge::{McpServerConfig, McpTransport};

const COMPLETE_VALUE: &str = "v1";
const REDACTED: &str = "[REDACTED]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum McpCredentialError {
    #[error("MCP server 配置无效")]
    InvalidConfig,
    #[error("MCP server 的敏感参数缺少取值")]
    MissingSecretArgumentValue,
    #[error("MCP server 参数疑似直接包含无法安全归类的凭据")]
    AmbiguousSecretArgument,
    #[error("MCP stdio 敏感参数禁止进入命令行，请改用明确的 secret env")]
    SecretArgumentUnsupported,
    #[error("MCP server 凭据定位符发生冲突")]
    DuplicateLocator,
    #[error("MCP server 安全凭据操作失败")]
    Credential,
    #[error("MCP server 安全凭据事务回滚失败")]
    RollbackFailed,
    #[error("MCP server 去密配置保存失败")]
    PersistFailed,
    #[error("MCP server 凭据不完整或尚未配置")]
    IncompleteCredentialSet,
}

impl McpCredentialError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfig => "MCP_CONFIG_INVALID",
            Self::MissingSecretArgumentValue => "MCP_SECRET_ARG_VALUE_MISSING",
            Self::AmbiguousSecretArgument => "MCP_SECRET_ARG_AMBIGUOUS",
            Self::SecretArgumentUnsupported => "MCP_STDIO_SECRET_ARG_FORBIDDEN_USE_SECRET_ENV",
            Self::DuplicateLocator => "MCP_CREDENTIAL_LOCATOR_COLLISION",
            Self::Credential => "MCP_CREDENTIAL_STORE_FAILED",
            Self::RollbackFailed => "MCP_CREDENTIAL_ROLLBACK_FAILED",
            Self::PersistFailed => "MCP_CONFIG_ATOMIC_PERSIST_FAILED",
            Self::IncompleteCredentialSet => "MCP_CREDENTIAL_SET_INCOMPLETE",
        }
    }
}

impl From<CredentialError> for McpCredentialError {
    fn from(_: CredentialError) -> Self {
        Self::Credential
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpSecretReference {
    pub locator: String,
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpStoredValue {
    Plain { value: String },
    Secret { credential: McpSecretReference },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpStoredArgument {
    Plain {
        value: String,
    },
    /// `prefix` is non-secret syntax such as `--token=`. It never contains the
    /// value held in the credential backend.
    Secret {
        prefix: String,
        credential: McpSecretReference,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpStoredTransport {
    Stdio {
        command: String,
        args: Vec<McpStoredArgument>,
        env: BTreeMap<String, McpSecretReference>,
    },
    Http {
        url: McpStoredValue,
        headers: BTreeMap<String, McpSecretReference>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpStoredServer {
    pub server_id: Uuid,
    pub name: String,
    pub transport: McpStoredTransport,
    pub enabled: bool,
    pub complete: McpSecretReference,
}

impl McpStoredServer {
    /// Renaming is metadata-only. Credential locators remain owned by the
    /// immutable UUID and therefore cannot be orphaned by a display-name edit.
    pub fn rename(&mut self, name: String) -> Result<(), McpCredentialError> {
        if name.trim().is_empty() {
            return Err(McpCredentialError::InvalidConfig);
        }
        self.name = name.trim().to_string();
        Ok(())
    }

    pub fn credential_locators(&self) -> Result<Vec<CredentialLocator>, McpCredentialError> {
        let mut ids = Vec::new();
        match &self.transport {
            McpStoredTransport::Stdio { args, env, .. } => {
                for argument in args {
                    if let McpStoredArgument::Secret { credential, .. } = argument {
                        ids.push(credential.locator.as_str());
                    }
                }
                ids.extend(env.values().map(|value| value.locator.as_str()));
            }
            McpStoredTransport::Http { url, headers } => {
                if let McpStoredValue::Secret { credential } = url {
                    ids.push(credential.locator.as_str());
                }
                ids.extend(headers.values().map(|value| value.locator.as_str()));
            }
        }
        // The complete marker is deleted first to make a partially deleted
        // server fail closed if another runtime races with deletion.
        ids.insert(0, self.complete.locator.as_str());
        let mut seen = HashSet::new();
        ids.into_iter()
            .map(|id| {
                if !seen.insert(id) {
                    return Err(McpCredentialError::DuplicateLocator);
                }
                parse_owned_locator(self.server_id, id)
            })
            .collect()
    }
}

struct PlannedWrite {
    locator: CredentialLocator,
    value: SecretValue,
}

struct PlannedServer {
    stored: McpStoredServer,
    writes: Vec<PlannedWrite>,
}

struct Snapshot {
    locator: CredentialLocator,
    previous: Option<SecretValue>,
}

/// Runtime-only hydrated MCP configuration. It is not serializable and its
/// debug representation cannot expose secret material.
pub struct ResolvedMcpServer {
    config: McpServerConfig,
    secret_values: Vec<SecretValue>,
    secret_url: bool,
    secret_arg_indexes: Vec<usize>,
}

impl ResolvedMcpServer {
    pub fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Apply both central pattern redaction and exact in-memory marker
    /// redaction before a third-party error reaches logs or the WebView.
    pub fn redact_error(&self, error: &str) -> String {
        let mut safe = crate::security::redaction::redact(error);
        for value in &self.secret_values {
            let exposed = value.expose();
            safe = safe.replace(exposed, REDACTED);
            if let Some((scheme, token)) = exposed.split_once(' ') {
                if matches!(scheme.to_ascii_lowercase().as_str(), "bearer" | "basic") {
                    safe = safe.replace(token, REDACTED);
                }
            }
            if let Some((_, query)) = exposed.split_once('?') {
                for item in query.split('&') {
                    if let Some((_, secret)) = item.split_once('=') {
                        if !secret.is_empty() {
                            safe = safe.replace(secret, REDACTED);
                        }
                    }
                }
            }
        }
        safe
    }
}

impl fmt::Debug for ResolvedMcpServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedMcpServer")
            .field("server_name", &self.config.name)
            .field("enabled", &self.config.enabled)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl Drop for ResolvedMcpServer {
    fn drop(&mut self) {
        match &mut self.config.transport {
            McpTransport::Stdio { args, env, .. } => {
                for index in &self.secret_arg_indexes {
                    if let Some(value) = args.get_mut(*index) {
                        value.zeroize();
                    }
                }
                for value in env.values_mut() {
                    value.zeroize();
                }
            }
            McpTransport::Http { url, headers } => {
                if self.secret_url {
                    url.zeroize();
                }
                for value in headers.values_mut() {
                    value.zeroize();
                }
            }
        }
    }
}

/// Migrates legacy plaintext configurations to UUID-owned credentials and
/// invokes `persist` only after every credential and the final complete marker
/// have been written and read back.
///
/// If either vault work or persistence fails, all vault entries are restored
/// to their original state. The persistence closure must itself use the
/// settings layer's atomic replace contract.
pub fn migrate_legacy_servers_with<B, F>(
    backend: &mut B,
    legacy: &[McpServerConfig],
    server_ids: &[Option<Uuid>],
    persist: F,
) -> Result<Vec<McpStoredServer>, McpCredentialError>
where
    B: CredentialBackend,
    F: FnOnce(&mut B, &[McpStoredServer]) -> Result<(), ()>,
{
    if legacy.len() != server_ids.len() {
        return Err(McpCredentialError::InvalidConfig);
    }
    let mut plans = Vec::with_capacity(legacy.len());
    let mut locators = HashSet::new();
    for (config, server_id) in legacy.iter().zip(server_ids.iter()) {
        let plan = plan_server(config, server_id.unwrap_or_else(Uuid::new_v4))?;
        for write in &plan.writes {
            if !locators.insert(write.locator.id().to_string()) {
                return Err(McpCredentialError::DuplicateLocator);
            }
        }
        plans.push(plan);
    }

    let mut snapshots = Vec::new();
    for plan in &plans {
        for write in &plan.writes {
            snapshots.push(Snapshot {
                locator: write.locator.clone(),
                previous: backend.get(&write.locator)?,
            });
        }
    }

    for plan in &plans {
        for write in &plan.writes {
            if let Err(error) =
                crate::credentials::replace_verified_with(backend, &write.locator, &write.value)
            {
                return Err(rollback_or_original(
                    backend,
                    &snapshots,
                    McpCredentialError::from(error),
                ));
            }
        }
    }

    let stored = plans
        .into_iter()
        .map(|plan| plan.stored)
        .collect::<Vec<_>>();
    if persist(backend, &stored).is_err() {
        return Err(rollback_or_original(
            backend,
            &snapshots,
            McpCredentialError::PersistFailed,
        ));
    }
    Ok(stored)
}

pub fn resolve_server_with<B: CredentialBackend>(
    backend: &mut B,
    stored: &McpStoredServer,
) -> Result<ResolvedMcpServer, McpCredentialError> {
    // Fail before touching the complete marker or any referenced credential.
    // This preserves legacy vault entries for an explicit user repair/delete
    // while guaranteeing a historical secret argument can never reach argv.
    validate_stored_stdio_arguments(stored)?;
    if !stored.complete.configured {
        return Err(McpCredentialError::IncompleteCredentialSet);
    }
    let complete_locator = parse_owned_locator(stored.server_id, &stored.complete.locator)?;
    if complete_locator.id()
        != mcp_locator(&stored.server_id.hyphenated().to_string(), "complete")?.id()
    {
        return Err(McpCredentialError::InvalidConfig);
    }
    let complete = backend
        .get(&complete_locator)?
        .ok_or(McpCredentialError::IncompleteCredentialSet)?;
    if complete.expose() != COMPLETE_VALUE {
        return Err(McpCredentialError::IncompleteCredentialSet);
    }

    let mut secret_values = vec![complete];
    let mut secret_url = false;
    let secret_arg_indexes = Vec::new();
    let transport = match &stored.transport {
        McpStoredTransport::Stdio { command, args, env } => {
            let mut runtime_args = Vec::with_capacity(args.len());
            for argument in args {
                match argument {
                    McpStoredArgument::Plain { value } => runtime_args.push(value.clone()),
                    McpStoredArgument::Secret { .. } => {
                        return Err(McpCredentialError::SecretArgumentUnsupported);
                    }
                }
            }
            let mut runtime_env = BTreeMap::new();
            for (name, reference) in env {
                let secret = resolve_reference(backend, stored.server_id, reference)?;
                runtime_env.insert(name.clone(), secret.expose().to_string());
                secret_values.push(secret);
            }
            McpTransport::Stdio {
                command: command.clone(),
                args: runtime_args,
                env: runtime_env,
            }
        }
        McpStoredTransport::Http { url, headers } => {
            let runtime_url = match url {
                McpStoredValue::Plain { value } => value.clone(),
                McpStoredValue::Secret { credential } => {
                    let secret = resolve_reference(backend, stored.server_id, credential)?;
                    let value = secret.expose().to_string();
                    secret_values.push(secret);
                    secret_url = true;
                    value
                }
            };
            let mut runtime_headers = BTreeMap::new();
            for (name, reference) in headers {
                let secret = resolve_reference(backend, stored.server_id, reference)?;
                runtime_headers.insert(name.clone(), secret.expose().to_string());
                secret_values.push(secret);
            }
            McpTransport::Http {
                url: runtime_url,
                headers: runtime_headers,
            }
        }
    };
    Ok(ResolvedMcpServer {
        config: McpServerConfig {
            name: stored.name.clone(),
            transport,
            enabled: stored.enabled,
        },
        secret_values,
        secret_url,
        secret_arg_indexes,
    })
}

/// Deletes the complete marker and every UUID-owned secret as one recoverable
/// cascade. A failure restores all entries to their prior values.
pub fn delete_server_credentials_with<B: CredentialBackend>(
    backend: &mut B,
    stored: &McpStoredServer,
) -> Result<(), McpCredentialError> {
    let locators = stored.credential_locators()?;
    let mut snapshots = Vec::with_capacity(locators.len());
    for locator in &locators {
        snapshots.push(Snapshot {
            locator: locator.clone(),
            previous: backend.get(locator)?,
        });
    }
    for locator in &locators {
        if let Err(error) = crate::credentials::delete_verified_with(backend, locator) {
            return Err(rollback_or_original(
                backend,
                &snapshots,
                McpCredentialError::from(error),
            ));
        }
    }
    Ok(())
}

/// Deletes every UUID-owned credential and persists removal of the safe
/// settings projection as one recoverable transaction.
pub fn delete_server_transaction_with<B, F>(
    backend: &mut B,
    stored: &McpStoredServer,
    persist: F,
) -> Result<(), McpCredentialError>
where
    B: CredentialBackend,
    F: FnOnce(&mut B) -> Result<(), ()>,
{
    let locators = stored.credential_locators()?;
    let mut snapshots = Vec::with_capacity(locators.len());
    for locator in &locators {
        snapshots.push(Snapshot {
            locator: locator.clone(),
            previous: backend.get(locator)?,
        });
    }
    for locator in &locators {
        if let Err(error) = crate::credentials::delete_verified_with(backend, locator) {
            return Err(rollback_or_original(
                backend,
                &snapshots,
                McpCredentialError::from(error),
            ));
        }
    }
    if persist(backend).is_err() {
        return Err(rollback_or_original(
            backend,
            &snapshots,
            McpCredentialError::PersistFailed,
        ));
    }
    Ok(())
}

fn plan_server(
    config: &McpServerConfig,
    server_id: Uuid,
) -> Result<PlannedServer, McpCredentialError> {
    config
        .validate()
        .map_err(|_| McpCredentialError::InvalidConfig)?;
    let owner = server_id.hyphenated().to_string();
    let mut writes = Vec::new();
    let transport = match &config.transport {
        McpTransport::Http { url, headers } => {
            let stored_url = if url_contains_secret(url) {
                let locator = mcp_locator(&owner, "secret-url")?;
                writes.push(secret_write(locator.clone(), url)?);
                McpStoredValue::Secret {
                    credential: configured_reference(&locator),
                }
            } else {
                McpStoredValue::Plain { value: url.clone() }
            };
            let mut stored_headers = BTreeMap::new();
            for (name, value) in headers {
                let slot = dynamic_slot("http-header", name);
                let locator = mcp_locator(&owner, &slot)?;
                writes.push(secret_write(locator.clone(), value)?);
                stored_headers.insert(name.clone(), configured_reference(&locator));
            }
            McpStoredTransport::Http {
                url: stored_url,
                headers: stored_headers,
            }
        }
        McpTransport::Stdio { command, args, env } => {
            let stored_args = plan_arguments(&owner, args, &mut writes)?;
            let mut stored_env = BTreeMap::new();
            for (name, value) in env {
                let slot = dynamic_slot("stdio-env", name);
                let locator = mcp_locator(&owner, &slot)?;
                writes.push(secret_write(locator.clone(), value)?);
                stored_env.insert(name.clone(), configured_reference(&locator));
            }
            McpStoredTransport::Stdio {
                command: command.clone(),
                args: stored_args,
                env: stored_env,
            }
        }
    };
    let complete_locator = mcp_locator(&owner, "complete")?;
    // Complete marker is intentionally last.
    writes.push(secret_write(complete_locator.clone(), COMPLETE_VALUE)?);
    Ok(PlannedServer {
        stored: McpStoredServer {
            server_id,
            name: config.name.clone(),
            transport,
            enabled: config.enabled,
            complete: configured_reference(&complete_locator),
        },
        writes,
    })
}

fn plan_arguments(
    _owner: &str,
    args: &[String],
    _writes: &mut Vec<PlannedWrite>,
) -> Result<Vec<McpStoredArgument>, McpCredentialError> {
    reject_secret_valued_stdio_args(args)?;
    Ok(args
        .iter()
        .cloned()
        .map(|value| McpStoredArgument::Plain { value })
        .collect())
}

/// Defense at the process boundary: secret-valued stdio arguments are never
/// supported because every argv value is observable through OS process tools.
/// Callers must place credentials in an explicitly named secret env entry.
pub(crate) fn reject_secret_valued_stdio_args(args: &[String]) -> Result<(), McpCredentialError> {
    for argument in args {
        if is_secret_flag(argument)
            || split_secret_assignment(argument).is_some()
            || url_contains_secret(argument)
        {
            return Err(McpCredentialError::SecretArgumentUnsupported);
        }
        if looks_like_bare_secret(argument) {
            return Err(McpCredentialError::AmbiguousSecretArgument);
        }
    }
    Ok(())
}

fn validate_stored_stdio_arguments(stored: &McpStoredServer) -> Result<(), McpCredentialError> {
    let McpStoredTransport::Stdio { args, .. } = &stored.transport else {
        return Ok(());
    };
    let mut plain = Vec::with_capacity(args.len());
    for argument in args {
        match argument {
            McpStoredArgument::Plain { value } => plain.push(value.clone()),
            McpStoredArgument::Secret { .. } => {
                return Err(McpCredentialError::SecretArgumentUnsupported);
            }
        }
    }
    reject_secret_valued_stdio_args(&plain)
}

fn secret_write(
    locator: CredentialLocator,
    value: &str,
) -> Result<PlannedWrite, McpCredentialError> {
    Ok(PlannedWrite {
        locator,
        value: SecretValue::new(value.to_string())?,
    })
}

fn configured_reference(locator: &CredentialLocator) -> McpSecretReference {
    McpSecretReference {
        locator: locator.id().to_string(),
        configured: true,
    }
}

fn resolve_reference<B: CredentialBackend>(
    backend: &mut B,
    server_id: Uuid,
    reference: &McpSecretReference,
) -> Result<SecretValue, McpCredentialError> {
    if !reference.configured {
        return Err(McpCredentialError::IncompleteCredentialSet);
    }
    let locator = parse_owned_locator(server_id, &reference.locator)?;
    backend
        .get(&locator)?
        .ok_or(McpCredentialError::IncompleteCredentialSet)
}

fn parse_locator(id: &str) -> Result<CredentialLocator, McpCredentialError> {
    let mut parts = id.split('/');
    let scope = parts.next().ok_or(McpCredentialError::InvalidConfig)?;
    let owner = parts.next().ok_or(McpCredentialError::InvalidConfig)?;
    let slot = parts.next().ok_or(McpCredentialError::InvalidConfig)?;
    if parts.next().is_some() {
        return Err(McpCredentialError::InvalidConfig);
    }
    CredentialLocator::new(scope, owner, slot).map_err(McpCredentialError::from)
}

fn parse_owned_locator(server_id: Uuid, id: &str) -> Result<CredentialLocator, McpCredentialError> {
    let locator = parse_locator(id)?;
    let owner = server_id.hyphenated().to_string();
    let expected_prefix = format!("mcp/{owner}/");
    if !locator.id().starts_with(&expected_prefix) {
        return Err(McpCredentialError::InvalidConfig);
    }
    Ok(locator)
}

fn mcp_locator(owner: &str, slot: &str) -> Result<CredentialLocator, McpCredentialError> {
    CredentialLocator::new("mcp", owner, slot).map_err(McpCredentialError::from)
}

fn dynamic_slot(kind: &str, name: &str) -> String {
    let canonical = name.trim().to_ascii_lowercase();
    let slug = canonical
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character)
            } else if matches!(character, '-' | '_') {
                Some('-')
            } else {
                None
            }
        })
        .take(16)
        .collect::<String>();
    let slug = if slug.is_empty() { "field" } else { &slug };
    let digest = Sha256::digest(canonical.as_bytes());
    format!("{kind}-{slug}-{}", hex_prefix(&digest, 12))
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
        if out.len() >= chars {
            out.truncate(chars);
            break;
        }
    }
    out
}

fn is_secret_flag(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "--token"
            | "--key"
            | "--api-key"
            | "--apikey"
            | "--secret"
            | "--password"
            | "--authorization"
    )
}

fn split_secret_assignment(value: &str) -> Option<(&str, &str)> {
    let (key, secret) = value.split_once('=')?;
    let normalized = key
        .trim_start_matches('-')
        .replace(['_', '-'], "")
        .to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "token" | "key" | "apikey" | "secret" | "password" | "authorization"
    ) {
        let prefix_len = value.len() - secret.len();
        Some((&value[..prefix_len], secret))
    } else {
        None
    }
}

fn url_contains_secret(value: &str) -> bool {
    let Some((_, after_scheme)) = value.split_once("://") else {
        return false;
    };
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    authority.contains('@')
        || value
            .split_once('?')
            .is_some_and(|(_, query)| !query.is_empty())
}

fn looks_like_bare_secret(value: &str) -> bool {
    let trimmed = value.trim();
    // Mixed alphanumeric positional values are ambiguous even when providers
    // issue short tokens. Fail closed; callers can use an explicit token flag
    // or environment variable so the secret slot is unambiguous.
    trimmed.len() >= 8
        && !trimmed.contains(['/', '\\', ':', '.', ' '])
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        && trimmed.bytes().any(|byte| byte.is_ascii_alphabetic())
        && trimmed.bytes().any(|byte| byte.is_ascii_digit())
}

fn rollback_or_original<B: CredentialBackend>(
    backend: &mut B,
    snapshots: &[Snapshot],
    original: McpCredentialError,
) -> McpCredentialError {
    if restore_snapshots(backend, snapshots).is_ok() {
        original
    } else {
        McpCredentialError::RollbackFailed
    }
}

fn restore_snapshots<B: CredentialBackend>(
    backend: &mut B,
    snapshots: &[Snapshot],
) -> Result<(), CredentialError> {
    for snapshot in snapshots.iter().rev() {
        match &snapshot.previous {
            Some(value) => backend.set(&snapshot.locator, value)?,
            None => backend.delete(&snapshot.locator)?,
        }
    }
    for snapshot in snapshots {
        let current = backend.get(&snapshot.locator)?;
        let matches = match (&snapshot.previous, current.as_ref()) {
            (None, None) => true,
            (Some(expected), Some(actual)) => expected.expose() == actual.expose(),
            _ => false,
        };
        if !matches {
            return Err(CredentialError::RollbackFailed);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemoryBackend {
        values: HashMap<String, String>,
        operation: usize,
        fail_on_operation: Option<usize>,
        set_order: Vec<String>,
        delete_order: Vec<String>,
    }

    impl MemoryBackend {
        fn maybe_fail(&mut self) -> Result<(), CredentialError> {
            self.operation += 1;
            if self.fail_on_operation == Some(self.operation) {
                Err(CredentialError::SecureStore)
            } else {
                Ok(())
            }
        }
    }

    impl CredentialBackend for MemoryBackend {
        fn set(
            &mut self,
            locator: &CredentialLocator,
            secret: &SecretValue,
        ) -> Result<(), CredentialError> {
            self.maybe_fail()?;
            self.set_order.push(locator.id().to_string());
            self.values
                .insert(locator.id().to_string(), secret.expose().to_string());
            Ok(())
        }

        fn get(
            &mut self,
            locator: &CredentialLocator,
        ) -> Result<Option<SecretValue>, CredentialError> {
            self.maybe_fail()?;
            self.values
                .get(locator.id())
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
            self.maybe_fail()?;
            self.delete_order.push(locator.id().to_string());
            self.values.remove(locator.id());
            Ok(())
        }
    }

    fn http_legacy(marker: &str) -> McpServerConfig {
        McpServerConfig {
            name: "元典".to_string(),
            enabled: true,
            transport: McpTransport::Http {
                url: format!("https://example.test/mcp?tenant={marker}"),
                headers: BTreeMap::from([
                    ("Authorization".to_string(), format!("Bearer {marker}")),
                    ("X-Api-Key".to_string(), marker.to_string()),
                ]),
            },
        }
    }

    fn stdio_legacy(marker: &str) -> McpServerConfig {
        McpServerConfig {
            name: "local".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "node".to_string(),
                args: vec!["server.js".to_string(), "--stdio".to_string()],
                env: BTreeMap::from([("API_KEY".to_string(), marker.to_string())]),
            },
        }
    }

    fn stdio_secret_arg(marker: &str) -> McpServerConfig {
        McpServerConfig {
            name: "unsafe-local".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "node".to_string(),
                args: vec![
                    "server.js".to_string(),
                    "--token".to_string(),
                    marker.to_string(),
                ],
                env: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn migration_serializes_references_only_and_runtime_resolves_all_values() {
        let marker = "mcp-secret-marker-123456";
        let id = Uuid::parse_str("10a9ee60-b989-4a17-8ad5-119d3a38ec55").unwrap();
        let legacy = vec![http_legacy(marker), stdio_legacy(marker)];
        let ids = vec![Some(id), None];
        let mut backend = MemoryBackend::default();

        let stored =
            migrate_legacy_servers_with(&mut backend, &legacy, &ids, |_, _| Ok(())).unwrap();
        let json = serde_json::to_string(&stored).unwrap();
        assert!(!json.contains(marker));
        assert!(json.contains("server_id"));
        assert!(json.contains("configured"));
        assert_eq!(stored[0].server_id, id);
        assert!(backend
            .set_order
            .last()
            .is_some_and(|locator| locator.ends_with("/complete")));

        let first = resolve_server_with(&mut backend, &stored[0]).unwrap();
        assert_eq!(first.config(), &legacy[0]);
        let second = resolve_server_with(&mut backend, &stored[1]).unwrap();
        assert_eq!(second.config(), &legacy[1]);
        assert!(!format!("{first:?}").contains(marker));
        assert!(!format!("{second:?}").contains(marker));
    }

    #[test]
    fn rename_keeps_uuid_and_all_credential_locators() {
        let marker = "rename-secret-marker-123";
        let id = Uuid::parse_str("ce548a74-8438-409f-9a7d-c73c0a1abb41").unwrap();
        let mut backend = MemoryBackend::default();
        let mut stored = migrate_legacy_servers_with(
            &mut backend,
            &[http_legacy(marker)],
            &[Some(id)],
            |_, _| Ok(()),
        )
        .unwrap()
        .remove(0);
        let before = stored
            .credential_locators()
            .unwrap()
            .into_iter()
            .map(|locator| locator.id().to_string())
            .collect::<Vec<_>>();

        stored.rename("重命名后的服务".to_string()).unwrap();

        let after = stored
            .credential_locators()
            .unwrap()
            .into_iter()
            .map(|locator| locator.id().to_string())
            .collect::<Vec<_>>();
        assert_eq!(stored.server_id, id);
        assert_eq!(before, after);
        assert!(resolve_server_with(&mut backend, &stored).is_ok());
    }

    #[test]
    fn persistence_failure_restores_preexisting_vault_values() {
        let marker = "persist-secret-marker-123";
        let id = Uuid::parse_str("dfc47c27-7c9d-4ed6-a85e-59de8e6cc3a6").unwrap();
        let old_locator = mcp_locator(&id.hyphenated().to_string(), "secret-url").unwrap();
        let mut backend = MemoryBackend::default();
        backend
            .values
            .insert(old_locator.id().to_string(), "old-value".to_string());

        let result = migrate_legacy_servers_with(
            &mut backend,
            &[http_legacy(marker)],
            &[Some(id)],
            |_, _| Err(()),
        );

        assert_eq!(result.unwrap_err(), McpCredentialError::PersistFailed);
        assert_eq!(
            backend.values.get(old_locator.id()).map(String::as_str),
            Some("old-value")
        );
        assert_eq!(backend.values.len(), 1);
    }

    #[test]
    fn mid_write_failure_restores_every_snapshot() {
        let marker = "write-failure-marker-123";
        let legacy = vec![http_legacy(marker)];
        let mut baseline = MemoryBackend::default();
        migrate_legacy_servers_with(&mut baseline, &legacy, &[None], |_, _| Ok(())).unwrap();
        let operation_count = baseline.operation;

        for fail_at in 1..=operation_count {
            let mut backend = MemoryBackend {
                fail_on_operation: Some(fail_at),
                ..MemoryBackend::default()
            };
            let before = backend.values.clone();
            let result = migrate_legacy_servers_with(&mut backend, &legacy, &[None], |_, _| Ok(()));
            if result.is_err() {
                assert_eq!(backend.values, before, "failure operation {fail_at}");
            }
        }
    }

    #[test]
    fn cascade_delete_removes_all_uuid_owned_credentials() {
        let marker = "delete-secret-marker-123";
        let mut backend = MemoryBackend::default();
        let stored =
            migrate_legacy_servers_with(&mut backend, &[stdio_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        assert!(!backend.values.is_empty());

        delete_server_credentials_with(&mut backend, &stored).unwrap();

        assert!(backend.values.is_empty());
        assert!(backend
            .delete_order
            .first()
            .is_some_and(|locator| locator.ends_with("/complete")));
    }

    #[test]
    fn cascade_delete_failure_restores_all_credentials() {
        let marker = "delete-rollback-marker-123";
        let mut backend = MemoryBackend::default();
        let stored =
            migrate_legacy_servers_with(&mut backend, &[stdio_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        let before = backend.values.clone();
        backend.operation = 0;
        // Snapshots consume N gets; fail on the second verified delete.
        backend.fail_on_operation = Some(stored.credential_locators().unwrap().len() + 4);

        let result = delete_server_credentials_with(&mut backend, &stored);

        assert!(result.is_err());
        assert_eq!(backend.values, before);
    }

    #[test]
    fn ambiguous_bare_argument_is_blocked_before_any_vault_write() {
        let marker = "sk-1234567890abcdef1234567890";
        let legacy = McpServerConfig {
            name: "ambiguous".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "node".to_string(),
                args: vec![marker.to_string()],
                env: BTreeMap::new(),
            },
        };
        let mut backend = MemoryBackend::default();

        let result = migrate_legacy_servers_with(&mut backend, &[legacy], &[None], |_, _| Ok(()));

        assert_eq!(
            result.unwrap_err(),
            McpCredentialError::AmbiguousSecretArgument
        );
        assert!(backend.values.is_empty());
    }

    #[test]
    fn secret_argument_migration_fails_before_vault_or_settings_persistence() {
        let marker = "stdio-secret-arg-marker";
        let mut backend = MemoryBackend::default();
        let existing_locator = mcp_locator("existing-owner", "stdio-env-api-key").unwrap();
        backend.values.insert(
            existing_locator.id().to_string(),
            "existing-value".to_string(),
        );
        let before = backend.values.clone();
        let mut persisted = false;

        let result = migrate_legacy_servers_with(
            &mut backend,
            &[stdio_secret_arg(marker)],
            &[None],
            |_, _| {
                persisted = true;
                Ok(())
            },
        );

        assert_eq!(
            result.unwrap_err(),
            McpCredentialError::SecretArgumentUnsupported
        );
        assert!(!persisted);
        assert_eq!(backend.values, before);
    }

    #[test]
    fn historical_stored_secret_argument_fails_before_reading_or_deleting_vault() {
        let id = Uuid::parse_str("1659cc26-54c9-45c0-bf67-088e9b1fde27").unwrap();
        let owner = id.hyphenated().to_string();
        let complete = mcp_locator(&owner, "complete").unwrap();
        let secret_arg = mcp_locator(&owner, "secret-arg-0001").unwrap();
        let stored = McpStoredServer {
            server_id: id,
            name: "historical".to_string(),
            enabled: true,
            transport: McpStoredTransport::Stdio {
                command: "node".to_string(),
                args: vec![McpStoredArgument::Secret {
                    prefix: "--token=".to_string(),
                    credential: configured_reference(&secret_arg),
                }],
                env: BTreeMap::new(),
            },
            complete: configured_reference(&complete),
        };
        let mut backend = MemoryBackend::default();
        backend
            .values
            .insert(complete.id().to_string(), COMPLETE_VALUE.to_string());
        backend
            .values
            .insert(secret_arg.id().to_string(), "historical-secret".to_string());
        let before = backend.values.clone();

        let result = resolve_server_with(&mut backend, &stored);

        assert_eq!(
            result.unwrap_err(),
            McpCredentialError::SecretArgumentUnsupported
        );
        assert_eq!(backend.operation, 0);
        assert!(backend.delete_order.is_empty());
        assert_eq!(backend.values, before);
    }

    #[test]
    fn short_mixed_positional_token_is_blocked_before_persistence() {
        let legacy = McpServerConfig {
            name: "short-token".to_string(),
            enabled: true,
            transport: McpTransport::Stdio {
                command: "node".to_string(),
                args: vec!["a1b2c3d4".to_string()],
                env: BTreeMap::new(),
            },
        };
        let mut backend = MemoryBackend::default();

        let result = migrate_legacy_servers_with(&mut backend, &[legacy], &[None], |_, _| Ok(()));

        assert_eq!(
            result.unwrap_err(),
            McpCredentialError::AmbiguousSecretArgument
        );
        assert!(backend.values.is_empty());
    }

    #[test]
    fn delete_persistence_failure_restores_all_credentials() {
        let marker = "delete-transaction-marker";
        let mut backend = MemoryBackend::default();
        let stored =
            migrate_legacy_servers_with(&mut backend, &[stdio_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        let before = backend.values.clone();

        let result = delete_server_transaction_with(&mut backend, &stored, |_| Err(()));

        assert_eq!(result.unwrap_err(), McpCredentialError::PersistFailed);
        assert_eq!(backend.values, before);
    }

    #[test]
    fn exact_runtime_marker_is_redacted_from_third_party_error() {
        let marker = "opaque-mcp-value-123456";
        let mut backend = MemoryBackend::default();
        let stored =
            migrate_legacy_servers_with(&mut backend, &[http_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        let resolved = resolve_server_with(&mut backend, &stored).unwrap();

        let safe = resolved.redact_error(&format!("remote echoed {marker}"));

        assert!(!safe.contains(marker));
        assert!(safe.contains(REDACTED));
    }

    #[test]
    fn bearer_token_and_secret_query_value_are_redacted_when_echoed_alone() {
        let token = "opaque-bearer-token-123";
        let query_secret = "tenant-secret-456";
        let legacy = McpServerConfig {
            name: "redaction".to_string(),
            enabled: true,
            transport: McpTransport::Http {
                url: format!("https://example.test/mcp?tenant={query_secret}"),
                headers: BTreeMap::from([("Authorization".to_string(), format!("Bearer {token}"))]),
            },
        };
        let mut backend = MemoryBackend::default();
        let stored = migrate_legacy_servers_with(&mut backend, &[legacy], &[None], |_, _| Ok(()))
            .unwrap()
            .remove(0);
        let resolved = resolve_server_with(&mut backend, &stored).unwrap();

        let safe = resolved.redact_error(&format!("echo {token} and {query_secret}"));

        assert!(!safe.contains(token));
        assert!(!safe.contains(query_secret));
    }

    #[test]
    fn missing_complete_marker_fails_closed() {
        let marker = "complete-marker-secret-123";
        let mut backend = MemoryBackend::default();
        let stored =
            migrate_legacy_servers_with(&mut backend, &[http_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        backend.values.remove(&stored.complete.locator);

        assert_eq!(
            resolve_server_with(&mut backend, &stored).unwrap_err(),
            McpCredentialError::IncompleteCredentialSet
        );
    }

    #[test]
    fn tampered_cross_owner_locator_cannot_resolve_or_delete() {
        let marker = "owner-isolation-secret-123";
        let mut backend = MemoryBackend::default();
        let mut stored =
            migrate_legacy_servers_with(&mut backend, &[http_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        let provider_locator = crate::credentials::StaticCredential::Mineru.locator();
        backend.values.insert(
            provider_locator.id().to_string(),
            "provider-secret".to_string(),
        );
        if let McpStoredTransport::Http { headers, .. } = &mut stored.transport {
            headers.get_mut("X-Api-Key").unwrap().locator = provider_locator.id().to_string();
        }
        let before = backend.values.clone();

        assert_eq!(
            resolve_server_with(&mut backend, &stored).unwrap_err(),
            McpCredentialError::InvalidConfig
        );
        assert_eq!(
            delete_server_credentials_with(&mut backend, &stored).unwrap_err(),
            McpCredentialError::InvalidConfig
        );
        assert_eq!(backend.values, before);
    }

    #[test]
    fn configured_false_reference_fails_closed() {
        let marker = "configured-state-secret-123";
        let mut backend = MemoryBackend::default();
        let mut stored =
            migrate_legacy_servers_with(&mut backend, &[http_legacy(marker)], &[None], |_, _| {
                Ok(())
            })
            .unwrap()
            .remove(0);
        stored.complete.configured = false;

        assert_eq!(
            resolve_server_with(&mut backend, &stored).unwrap_err(),
            McpCredentialError::IncompleteCredentialSet
        );
    }

    #[test]
    fn locator_hashes_are_stable_and_do_not_contain_field_values() {
        let one = dynamic_slot("http-header", "Authorization");
        let two = dynamic_slot("http-header", "authorization");
        assert_eq!(one, two);
        assert!(one.starts_with("http-header-authorization-"));
        assert!(!one.contains("Bearer"));
    }
}
