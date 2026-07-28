use serde::Serialize;

use crate::credentials::{
    delete_verified_with, replace_verified_with, CredentialBackend, CredentialError,
    CredentialLocator, SecretValue,
};

use super::TeamIdentity;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TeamPublicIdentity {
    pub team_id: String,
    pub team_name: String,
    pub member_id: String,
    pub my_name: String,
    pub role: String,
}

impl From<&TeamIdentity> for TeamPublicIdentity {
    fn from(identity: &TeamIdentity) -> Self {
        Self {
            team_id: identity.team_id.clone(),
            team_name: identity.team_name.clone(),
            member_id: identity.member_id.clone(),
            my_name: identity.my_name.clone(),
            role: identity.role.clone(),
        }
    }
}

fn locator(team_id: &str, slot: &str) -> Result<CredentialLocator, CredentialError> {
    CredentialLocator::new("team", team_id, slot)
}

pub fn secret_locator(team_id: &str) -> Result<CredentialLocator, CredentialError> {
    locator(team_id, "shared-secret")
}

pub fn pairing_locator(team_id: &str) -> Result<CredentialLocator, CredentialError> {
    locator(team_id, "pairing-code")
}

fn restore<B: CredentialBackend>(
    backend: &mut B,
    snapshots: &[(CredentialLocator, Option<SecretValue>)],
) -> bool {
    let mut complete = true;
    for (locator, value) in snapshots {
        complete &= match value {
            Some(value) => backend.set(locator, value).is_ok(),
            None => backend.delete(locator).is_ok(),
        };
    }
    complete
}

pub fn migrate_legacy_identity_with<B, F>(
    identity: &TeamIdentity,
    backend: &mut B,
    persist_sanitized: F,
) -> Result<TeamIdentity, String>
where
    B: CredentialBackend,
    F: FnOnce(&mut B, &TeamIdentity) -> Result<(), String>,
{
    let secret_locator = secret_locator(&identity.team_id).map_err(|e| e.code().to_string())?;
    let pairing_locator = pairing_locator(&identity.team_id).map_err(|e| e.code().to_string())?;
    let updates = [
        (secret_locator.clone(), identity.team_secret.as_str()),
        (
            pairing_locator.clone(),
            identity.pairing_code.as_deref().unwrap_or(""),
        ),
    ]
    .into_iter()
    .filter(|(_, value)| !value.trim().is_empty())
    .collect::<Vec<_>>();
    let mut snapshots = Vec::with_capacity(updates.len());
    for (locator, _) in &updates {
        snapshots.push((
            locator.clone(),
            backend
                .get(locator)
                .map_err(|error| error.code().to_string())?,
        ));
    }
    for (locator, value) in &updates {
        let value = SecretValue::new((*value).to_string()).map_err(|e| e.code().to_string())?;
        if let Err(error) = replace_verified_with(backend, locator, &value) {
            let restored = restore(backend, &snapshots);
            return Err(if restored {
                error.code().to_string()
            } else {
                CredentialError::RollbackFailed.code().to_string()
            });
        }
    }
    let mut sanitized = identity.clone();
    sanitized.team_secret.clear();
    sanitized.pairing_code = None;
    if let Err(error) = persist_sanitized(backend, &sanitized) {
        if !restore(backend, &snapshots) {
            return Err(CredentialError::RollbackFailed.code().to_string());
        }
        return Err(error);
    }
    Ok(sanitized)
}

pub fn resolve_runtime_identity(sanitized: &TeamIdentity) -> Result<TeamIdentity, String> {
    resolve_runtime_identity_with(sanitized, &mut crate::credentials::SystemCredentialBackend)
}

pub fn read_runtime_identity() -> Result<Option<TeamIdentity>, String> {
    crate::settings::read_settings()?
        .team
        .as_ref()
        .map(resolve_runtime_identity)
        .transpose()
}

pub fn persist_identity(identity: &TeamIdentity) -> Result<TeamIdentity, String> {
    let mut settings = crate::settings::read_settings()?;
    let mut backend = crate::credentials::SystemCredentialBackend;
    migrate_legacy_identity_with(identity, &mut backend, |backend, sanitized| {
        settings.team = Some(sanitized.clone());
        crate::settings::write_settings_using_backend(&settings, backend)
    })
}

pub fn replace_pairing_code(team_id: &str, code: &str) -> Result<(), String> {
    let value = SecretValue::new(code.to_string()).map_err(|error| error.code().to_string())?;
    replace_verified_with(
        &mut crate::credentials::SystemCredentialBackend,
        &pairing_locator(team_id).map_err(|error| error.code().to_string())?,
        &value,
    )
    .map_err(|error| error.code().to_string())
}

pub fn delete_identity_credentials(team_id: &str) -> Result<(), String> {
    delete_identity_credentials_with(team_id, &mut crate::credentials::SystemCredentialBackend)
}

pub fn resolve_runtime_identity_with<B: CredentialBackend>(
    sanitized: &TeamIdentity,
    backend: &mut B,
) -> Result<TeamIdentity, String> {
    let mut identity = sanitized.clone();
    identity.team_secret = backend
        .get(&secret_locator(&identity.team_id).map_err(|e| e.code().to_string())?)
        .map_err(|error| error.code().to_string())?
        .ok_or("TEAM_SECRET_NOT_CONFIGURED")?
        .into_string();
    identity.pairing_code = backend
        .get(&pairing_locator(&identity.team_id).map_err(|e| e.code().to_string())?)
        .map_err(|error| error.code().to_string())?
        .map(SecretValue::into_string);
    Ok(identity)
}

fn delete_identity_credentials_then_with<B, F>(
    team_id: &str,
    backend: &mut B,
    persist: F,
) -> Result<(), String>
where
    B: CredentialBackend,
    F: FnOnce(&mut B) -> Result<(), String>,
{
    let locators = [
        secret_locator(team_id).map_err(|e| e.code().to_string())?,
        pairing_locator(team_id).map_err(|e| e.code().to_string())?,
    ];
    let mut snapshots = Vec::with_capacity(locators.len());
    for locator in &locators {
        snapshots.push((
            locator.clone(),
            backend
                .get(locator)
                .map_err(|error| error.code().to_string())?,
        ));
    }
    for locator in &locators {
        if let Err(error) = delete_verified_with(backend, locator) {
            let restored = restore(backend, &snapshots);
            return Err(if restored {
                error.code().to_string()
            } else {
                CredentialError::RollbackFailed.code().to_string()
            });
        }
    }
    if let Err(error) = persist(backend) {
        if !restore(backend, &snapshots) {
            return Err(CredentialError::RollbackFailed.code().to_string());
        }
        return Err(error);
    }
    Ok(())
}

pub fn delete_identity_credentials_with<B: CredentialBackend>(
    team_id: &str,
    backend: &mut B,
) -> Result<(), String> {
    delete_identity_credentials_then_with(team_id, backend, |_| Ok(()))
}

pub fn clear_persisted_identity(team_id: &str) -> Result<(), String> {
    let mut settings = crate::settings::read_settings()?;
    let mut backend = crate::credentials::SystemCredentialBackend;
    delete_identity_credentials_then_with(team_id, &mut backend, |backend| {
        settings.team = None;
        crate::settings::write_settings_using_backend(&settings, backend)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryBackend {
        values: HashMap<String, String>,
    }

    impl CredentialBackend for MemoryBackend {
        fn set(
            &mut self,
            locator: &CredentialLocator,
            secret: &SecretValue,
        ) -> Result<(), CredentialError> {
            self.values
                .insert(locator.id().to_string(), secret.expose().to_string());
            Ok(())
        }

        fn get(
            &mut self,
            locator: &CredentialLocator,
        ) -> Result<Option<SecretValue>, CredentialError> {
            self.values
                .get(locator.id())
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
            self.values.remove(locator.id());
            Ok(())
        }
    }

    fn identity() -> TeamIdentity {
        TeamIdentity {
            team_id: "11111111-2222-3333-4444-555555555555".into(),
            team_name: "Test Team".into(),
            team_secret: "team-secret-marker".into(),
            member_id: "member-1".into(),
            my_name: "Alice".into(),
            role: "leader".into(),
            pairing_code: Some("123456".into()),
        }
    }

    #[test]
    fn public_identity_never_serializes_secret_or_pairing_code() {
        let identity = identity();
        let json = serde_json::to_string(&TeamPublicIdentity::from(&identity)).expect("json");
        let stored_json = serde_json::to_string(&identity).expect("stored json");
        assert!(!json.contains("team-secret-marker"));
        assert!(!json.contains("123456"));
        assert!(!json.contains("team_secret"));
        assert!(!json.contains("pairing_code"));
        assert!(!stored_json.contains("team-secret-marker"));
        assert!(!stored_json.contains("123456"));
        assert!(!stored_json.contains("team_secret"));
        assert!(!stored_json.contains("pairing_code"));
        let debug = format!("{identity:?}");
        assert!(!debug.contains("team-secret-marker"));
        assert!(!debug.contains("123456"));
    }

    #[test]
    fn migration_persists_credentials_then_returns_sanitized_identity() {
        let identity = identity();
        let mut backend = MemoryBackend::default();
        let sanitized =
            migrate_legacy_identity_with(&identity, &mut backend, |_, _| Ok(())).expect("migrate");
        assert!(sanitized.team_secret.is_empty());
        assert!(sanitized.pairing_code.is_none());
        let runtime =
            resolve_runtime_identity_with(&sanitized, &mut backend).expect("runtime identity");
        assert_eq!(runtime.team_secret, "team-secret-marker");
        assert_eq!(runtime.pairing_code.as_deref(), Some("123456"));
    }

    #[test]
    fn persistence_failure_rolls_back_credentials() {
        let identity = identity();
        let mut backend = MemoryBackend::default();
        backend.values.insert(
            secret_locator(&identity.team_id).unwrap().id().to_string(),
            "old-secret".into(),
        );
        assert_eq!(
            migrate_legacy_identity_with(&identity, &mut backend, |_, _| {
                Err("PERSIST_FAILED".into())
            }),
            Err("PERSIST_FAILED".into())
        );
        assert_eq!(
            backend
                .values
                .get(secret_locator(&identity.team_id).unwrap().id())
                .map(String::as_str),
            Some("old-secret")
        );
    }

    #[test]
    fn leave_or_kick_deletes_secret_and_pairing_code() {
        let identity = identity();
        let mut backend = MemoryBackend::default();
        migrate_legacy_identity_with(&identity, &mut backend, |_, _| Ok(())).expect("migrate");

        delete_identity_credentials_with(&identity.team_id, &mut backend).expect("delete");

        assert!(backend.values.is_empty());
    }

    #[test]
    fn local_clear_persistence_failure_restores_both_credentials() {
        let identity = identity();
        let mut backend = MemoryBackend::default();
        migrate_legacy_identity_with(&identity, &mut backend, |_, _| Ok(())).expect("migrate");

        assert_eq!(
            delete_identity_credentials_then_with(&identity.team_id, &mut backend, |_| Err(
                "PERSIST_FAILED".into()
            ),),
            Err("PERSIST_FAILED".into())
        );
        assert_eq!(
            backend
                .values
                .get(secret_locator(&identity.team_id).unwrap().id())
                .map(String::as_str),
            Some("team-secret-marker")
        );
        assert_eq!(
            backend
                .values
                .get(pairing_locator(&identity.team_id).unwrap().id())
                .map(String::as_str),
            Some("123456")
        );
    }
}
