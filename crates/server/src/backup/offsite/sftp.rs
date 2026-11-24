//! Destination SFTP : une session SSH, le sous-système `sftp`, et le client
//! de [`super::sftp_proto`].
//!
//! # Clé d'hôte
//!
//! Jamais de confiance au premier usage : sans empreinte confirmée par
//! l'administrateur, la connexion est coupée **avant toute
//! authentification** et l'empreinte vue est rendue, pour qu'il la compare
//! à celle du serveur (`ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub`).
//! Une empreinte différente de celle confirmée est refusée de même : aucun
//! mot de passe, aucune sauvegarde ne part vers un imposteur.
//!
//! # Clé privée
//!
//! Ed25519 ou ECDSA. Les clés RSA sont refusées : la crate `rsa` que russh
//! tire est sujette à RUSTSEC-2023-0071 pour les opérations à clé privée, et
//! DumbMonit ne s'en sert que pour vérifier des clés d'hôte (voir
//! `deny.toml`).
//!
//! # Écriture
//!
//! Le fichier part sous `nom.part`, puis est renommé : une copie
//! interrompue ne ressemble jamais à une sauvegarde complète.

use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};

use super::sftp_proto::{self, FX_NO_SUCH_FILE, FX_PERMISSION_DENIED, Sftp};
use super::transfer::{
    self, CONNECT_TIMEOUT, Failure, HostKey, READ_TIMEOUT, SHORT_TIMEOUT, TRANSFER_TIMEOUT,
};
use super::{Secrets, SftpSettings, check_name};

type Stream = Pin<Box<russh::ChannelStream<client::Msg>>>;

/// Secrets d'une connexion.
#[derive(Clone, Default)]
pub struct Credentials {
    pub password: Option<String>,
    pub private_key: Option<String>,
    pub key_passphrase: Option<String>,
}

impl Credentials {
    pub fn from_secrets(secrets: &Secrets) -> Self {
        let get = |key: &str| secrets.get(key).filter(|value| !value.is_empty()).cloned();
        Self {
            password: get("password"),
            private_key: get("private_key"),
            key_passphrase: get("key_passphrase"),
        }
    }
}

/// Vérification de la clé d'hôte contre l'empreinte épinglée.
struct Pinned {
    pinned: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Pinned {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fingerprint = fingerprint(server_public_key);
        let accepted = self.pinned.as_deref() == Some(fingerprint.as_str());
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(fingerprint);
        }
        Ok(accepted)
    }
}

/// Empreinte SHA-256 au format d'OpenSSH : `SHA256:…`.
fn fingerprint(key: &PublicKeyOrCertificate) -> String {
    let data = match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => key.key_data(),
        PublicKeyOrCertificate::Certificate(certificate) => certificate.public_key(),
    };
    data.fingerprint(HashAlg::Sha256).to_string()
}

fn load_key(pem: &str, passphrase: Option<&str>) -> Result<russh::keys::PrivateKey, String> {
    let passphrase = passphrase.filter(|passphrase| !passphrase.is_empty());
    let key = russh::keys::decode_secret_key(pem, passphrase).map_err(|error| {
        format!(
            "The private key cannot be read ({error}). Paste the whole OpenSSH key, and its \
             passphrase if it has one."
        )
    })?;
    if key.algorithm().is_rsa() {
        return Err(
            "RSA keys are not accepted: use an Ed25519 or ECDSA key (ssh-keygen -t ed25519)."
                .into(),
        );
    }
    Ok(key)
}

/// Contrôle d'une clé privée à l'enregistrement : lisible, et pas RSA.
pub fn check_private_key(pem: &str, passphrase: Option<&str>) -> Result<(), String> {
    load_key(pem, passphrase).map(|_| ())
}

fn ssh_failure(error: russh::Error) -> Failure {
    match error {
        russh::Error::IO(error) => Failure::transient(format!("SFTP connection: {error}")),
        russh::Error::ConnectionTimeout | russh::Error::InactivityTimeout => {
            Failure::transient(format!("SFTP connection: {error}"))
        }
        other => Failure::fatal(format!("SSH: {other}")),
    }
}

fn host_key_failure(settings: &SftpSettings, seen: String) -> Failure {
    let changed = settings.host_key.is_some();
    let message = if changed {
        format!(
            "The SFTP server presented another host key ({seen}) than the one you confirmed. \
             Nothing was sent. If the server was reinstalled, confirm the new key; otherwise \
             something is impersonating it."
        )
    } else {
        format!(
            "The host key of the SFTP server is not confirmed yet. Nothing was sent. Check that \
             the server's fingerprint is {seen}, then confirm it."
        )
    };
    Failure { message, retry: false, host_key: Some(HostKey { fingerprint: seen, changed }) }
}

fn sftp_failure(what: &str, error: sftp_proto::Error) -> Failure {
    match &error {
        sftp_proto::Error::Io(_) => Failure::transient(format!("{what}: {error}")),
        sftp_proto::Error::Status { code: FX_NO_SUCH_FILE, .. } => {
            Failure::fatal(format!("{what}: {error}. Check that the remote folder exists."))
        }
        sftp_proto::Error::Status { code: FX_PERMISSION_DENIED, .. } => {
            Failure::fatal(format!("{what}: {error}. Check the rights of the account."))
        }
        _ => Failure::fatal(format!("{what}: {error}")),
    }
}

fn too_slow(what: &str) -> Failure {
    Failure::transient(format!("{what}: the SFTP server stopped answering"))
}

/// `dossier/nom`, le dossier de connexion quand le dossier est vide.
fn join(directory: &str, name: &str) -> String {
    match directory {
        "" => name.to_string(),
        "/" => format!("/{name}"),
        directory => format!("{directory}/{name}"),
    }
}

struct Session {
    handle: client::Handle<Pinned>,
    sftp: Sftp<Stream>,
}

async fn connect(settings: &SftpSettings, credentials: &Credentials) -> Result<Session, Failure> {
    let seen = Arc::new(Mutex::new(None));
    let handler = Pinned { pinned: settings.host_key.clone(), seen: seen.clone() };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(READ_TIMEOUT),
        ..client::Config::default()
    });
    let address = (settings.host.as_str(), settings.port);
    let connected =
        tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, address, handler)).await;
    let seen_key = seen.lock().ok().and_then(|seen| seen.clone());
    let mut handle = match connected {
        Err(_) => {
            return Err(Failure::transient(format!(
                "SFTP: no answer from {}:{} within {}s",
                settings.host,
                settings.port,
                CONNECT_TIMEOUT.as_secs()
            )));
        }
        Ok(Err(error)) => {
            if let Some(seen) = seen_key
                && settings.host_key.as_deref() != Some(seen.as_str())
            {
                return Err(host_key_failure(settings, seen));
            }
            return Err(ssh_failure(error));
        }
        Ok(Ok(handle)) => handle,
    };
    // Garde redondante : une clé acceptée est forcément l'épinglée.
    if seen_key.is_none() || seen_key != settings.host_key {
        return Err(Failure::fatal("SFTP: the server presented no host key we confirmed"));
    }

    authenticate(&mut handle, settings, credentials).await?;

    let opening = "opening the SFTP session";
    let channel = tokio::time::timeout(CONNECT_TIMEOUT, handle.channel_open_session())
        .await
        .map_err(|_| too_slow(opening))?
        .map_err(ssh_failure)?;
    tokio::time::timeout(CONNECT_TIMEOUT, channel.request_subsystem(true, "sftp"))
        .await
        .map_err(|_| too_slow(opening))?
        .map_err(ssh_failure)?;
    let stream: Stream = Box::pin(channel.into_stream());
    let sftp = tokio::time::timeout(CONNECT_TIMEOUT, Sftp::init(stream))
        .await
        .map_err(|_| too_slow(opening))?
        .map_err(|error| sftp_failure(opening, error))?;
    Ok(Session { handle, sftp })
}

/// Clé privée d'abord quand il y en a une, puis mot de passe.
async fn authenticate(
    handle: &mut client::Handle<Pinned>,
    settings: &SftpSettings,
    credentials: &Credentials,
) -> Result<(), Failure> {
    let user = settings.username.as_str();
    let what = "SFTP authentication";
    if let Some(pem) = &credentials.private_key {
        let key = load_key(pem, credentials.key_passphrase.as_deref()).map_err(Failure::fatal)?;
        let key = PrivateKeyWithHashAlg::new(Arc::new(key), None);
        let attempt = handle.authenticate_publickey(user, key);
        let result = tokio::time::timeout(CONNECT_TIMEOUT, attempt)
            .await
            .map_err(|_| too_slow(what))?
            .map_err(ssh_failure)?;
        if result.success() {
            return Ok(());
        }
    }
    if let Some(password) = &credentials.password {
        let attempt = handle.authenticate_password(user, password.as_str());
        let result = tokio::time::timeout(CONNECT_TIMEOUT, attempt)
            .await
            .map_err(|_| too_slow(what))?
            .map_err(ssh_failure)?;
        if result.success() {
            return Ok(());
        }
    }
    Err(Failure::fatal("The SFTP server refused the user name, the password or the key."))
}

/// Écrit sous `partial`, puis renomme en `target`.
async fn write_file(
    sftp: &mut Sftp<Stream>,
    partial: &str,
    target: &str,
    file: &mut tokio::fs::File,
) -> sftp_proto::Result<u64> {
    let handle = sftp.open_write(partial).await?;
    let written = sftp.write_from(&handle, file).await;
    let closed = sftp.close(&handle).await;
    let written = written?;
    closed?;
    // Un fichier du même nom (envoi précédent interrompu après le renommage)
    // est remplacé : SFTP v3 refuse de renommer sur un fichier existant.
    let _ = sftp.remove(target).await;
    sftp.rename(partial, target).await?;
    Ok(written)
}

/// Une destination SFTP ; la session s'ouvre au premier besoin et sert à
/// toutes les opérations suivantes.
pub struct Client {
    settings: SftpSettings,
    credentials: Credentials,
    session: Option<Session>,
}

impl Client {
    pub fn new(settings: SftpSettings, credentials: Credentials) -> Self {
        Self { settings, credentials, session: None }
    }

    /// Oublie la session : la prochaine opération se reconnecte.
    pub fn reset(&mut self) {
        self.session = None;
    }

    pub async fn close(mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.handle.disconnect(russh::Disconnect::ByApplication, "", "en").await;
        }
    }

    async fn session(&mut self) -> Result<&mut Session, Failure> {
        if self.session.is_none() {
            self.session = Some(connect(&self.settings, &self.credentials).await?);
        }
        Ok(self.session.as_mut().expect("session just opened"))
    }

    pub async fn put_file(&mut self, name: &str, path: &Path) -> Result<u64, Failure> {
        check_name(name)?;
        let target = join(&self.settings.directory, name);
        let partial = format!("{target}.part");
        let what = format!("uploading {name}");
        let mut file = transfer::open(path).await?;
        let session = self.session().await?;
        let written = write_file(&mut session.sftp, &partial, &target, &mut file);
        match tokio::time::timeout(TRANSFER_TIMEOUT, written).await {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(error)) => {
                let cleanup = session.sftp.remove(&partial);
                let _ = tokio::time::timeout(SHORT_TIMEOUT, cleanup).await;
                let failure = sftp_failure(&what, error);
                if failure.retry {
                    self.reset();
                }
                Err(failure)
            }
            Err(_) => {
                self.reset();
                Err(too_slow(&what))
            }
        }
    }

    pub async fn put_bytes(&mut self, name: &str, data: Vec<u8>) -> Result<(), Failure> {
        check_name(name)?;
        let target = join(&self.settings.directory, name);
        let what = format!("writing {name}");
        let session = self.session().await?;
        let written = async {
            let handle = session.sftp.open_write(&target).await?;
            let written = session.sftp.write_from(&handle, &mut data.as_slice()).await;
            let closed = session.sftp.close(&handle).await;
            written?;
            closed
        };
        match tokio::time::timeout(SHORT_TIMEOUT, written).await {
            Ok(result) => result.map_err(|error| sftp_failure(&what, error)),
            Err(_) => {
                self.reset();
                Err(too_slow(&what))
            }
        }
    }

    pub async fn list(&mut self) -> Result<Vec<String>, Failure> {
        let directory = match self.settings.directory.as_str() {
            "" => ".".to_string(),
            directory => directory.to_string(),
        };
        let what = "listing the remote folder";
        let session = self.session().await?;
        match tokio::time::timeout(SHORT_TIMEOUT, session.sftp.list_dir(&directory)).await {
            Ok(result) => result.map_err(|error| sftp_failure(what, error)),
            Err(_) => {
                self.reset();
                Err(too_slow(what))
            }
        }
    }

    pub async fn delete(&mut self, name: &str) -> Result<(), Failure> {
        check_name(name)?;
        let target = join(&self.settings.directory, name);
        let what = format!("deleting {name}");
        let session = self.session().await?;
        match tokio::time::timeout(SHORT_TIMEOUT, session.sftp.remove(&target)).await {
            Ok(Ok(())) => Ok(()),
            // Déjà parti : c'est ce qu'on voulait.
            Ok(Err(sftp_proto::Error::Status { code: FX_NO_SUCH_FILE, .. })) => Ok(()),
            Ok(Err(error)) => Err(sftp_failure(&what, error)),
            Err(_) => {
                self.reset();
                Err(too_slow(&what))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_paths_are_joined_plainly() {
        assert_eq!(join("", "a.db"), "a.db");
        assert_eq!(join("/", "a.db"), "/a.db");
        assert_eq!(join("/volume1/backups", "a.db"), "/volume1/backups/a.db");
        assert_eq!(join("backups", "a.db"), "backups/a.db");
    }

    #[test]
    fn an_unreadable_key_is_refused_with_a_message() {
        let error = check_private_key("not a key", None).unwrap_err();
        assert!(error.starts_with("The private key cannot be read"), "{error}");
    }

    #[test]
    fn credentials_ignore_empty_secrets() {
        let secrets: Secrets = [("password".to_string(), String::new())].into();
        assert!(Credentials::from_secrets(&secrets).password.is_none());
    }

    #[test]
    fn a_changed_key_is_reported_as_such() {
        let mut settings = SftpSettings { host: "h".into(), port: 22, ..SftpSettings::default() };
        let failure = host_key_failure(&settings, "SHA256:new".into());
        assert_eq!(failure.host_key.as_ref().map(|key| key.changed), Some(false));
        assert!(!failure.retry);
        settings.host_key = Some("SHA256:old".into());
        let failure = host_key_failure(&settings, "SHA256:new".into());
        assert_eq!(
            failure.host_key,
            Some(HostKey { fingerprint: "SHA256:new".into(), changed: true })
        );
    }
}
