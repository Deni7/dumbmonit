//! Client SFTP version 3 (draft-ietf-secsh-filexfer-02), réduit à ce qu'une
//! copie de sauvegarde demande : écrire un fichier, le renommer, lister un
//! dossier, supprimer.
//!
//! Le protocole passe sur n'importe quel flux (`AsyncRead + AsyncWrite`) :
//! en production le canal SSH du sous-système `sftp`, dans les tests un faux
//! serveur en mémoire. Les écritures sont envoyées par fenêtre de plusieurs
//! requêtes en vol : attendre chaque accusé ferait du débit une affaire de
//! latence (32 Kio par aller-retour).

use std::collections::VecDeque;
use std::fmt;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const FXP_INIT: u8 = 1;
pub const FXP_VERSION: u8 = 2;
pub const FXP_OPEN: u8 = 3;
pub const FXP_CLOSE: u8 = 4;
pub const FXP_WRITE: u8 = 6;
pub const FXP_OPENDIR: u8 = 11;
pub const FXP_READDIR: u8 = 12;
pub const FXP_REMOVE: u8 = 13;
pub const FXP_RENAME: u8 = 18;
pub const FXP_STATUS: u8 = 101;
pub const FXP_HANDLE: u8 = 102;
pub const FXP_NAME: u8 = 104;

pub const FX_OK: u32 = 0;
pub const FX_EOF: u32 = 1;
pub const FX_NO_SUCH_FILE: u32 = 2;
pub const FX_PERMISSION_DENIED: u32 = 3;

const FXF_WRITE: u32 = 0x02;
const FXF_CREAT: u32 = 0x08;
const FXF_TRUNC: u32 = 0x10;

const ATTR_SIZE: u32 = 0x01;
const ATTR_UIDGID: u32 = 0x02;
const ATTR_PERMISSIONS: u32 = 0x04;
const ATTR_ACMODTIME: u32 = 0x08;
const ATTR_EXTENDED: u32 = 0x8000_0000;

/// Données par requête d'écriture : sous la limite que tout serveur accepte
/// (OpenSSH prend jusqu'à 256 Kio, la norme garantit 32 Kio).
const WRITE_CHUNK: usize = 32 * 1024;
/// Écritures en vol avant d'attendre un accusé.
const WINDOW: usize = 16;
/// Plus grand paquet accepté du serveur.
const MAX_PACKET: usize = 1024 * 1024;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    /// Le serveur a répondu par un statut d'erreur.
    Status {
        code: u32,
        message: String,
    },
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "SFTP: {error}"),
            Self::Status { code, message } => {
                let meaning = match *code {
                    FX_NO_SUCH_FILE => "no such file or folder",
                    FX_PERMISSION_DENIED => "permission denied",
                    _ => "failure",
                };
                if message.trim().is_empty() {
                    write!(f, "SFTP: {meaning}")
                } else {
                    write!(f, "SFTP: {meaning} ({})", message.trim())
                }
            }
            Self::Protocol(message) => write!(f, "SFTP: {message}"),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// --------------------------------------------------------------------------
// Encodage
// --------------------------------------------------------------------------

/// Paquet en construction : type, puis champs.
pub struct Packet(Vec<u8>);

impl Packet {
    pub fn new(kind: u8) -> Self {
        Self(vec![kind])
    }

    pub fn u32(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn u64(mut self, value: u64) -> Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub fn bytes(mut self, value: &[u8]) -> Self {
        self.0.extend_from_slice(&(value.len() as u32).to_be_bytes());
        self.0.extend_from_slice(value);
        self
    }

    pub fn string(self, value: &str) -> Self {
        self.bytes(value.as_bytes())
    }

    /// Le paquet sur le fil : longueur, puis contenu.
    pub fn framed(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.0.len() + 4);
        out.extend_from_slice(&(self.0.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.0);
        out
    }
}

/// Lecture des champs d'un paquet reçu.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| Error::Protocol("truncated packet".into()))?;
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let raw = self.take(4)?;
        Ok(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
    }

    pub fn u64(&mut self) -> Result<u64> {
        let high = u64::from(self.u32()?);
        let low = u64::from(self.u32()?);
        Ok((high << 32) | low)
    }

    pub fn bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.u32()? as usize;
        self.take(len)
    }

    pub fn string(&mut self) -> Result<String> {
        Ok(String::from_utf8_lossy(self.bytes()?).into_owned())
    }

    /// Passe une structure ATTRS.
    pub fn skip_attrs(&mut self) -> Result<()> {
        let flags = self.u32()?;
        if flags & ATTR_SIZE != 0 {
            self.u64()?;
        }
        if flags & ATTR_UIDGID != 0 {
            self.u32()?;
            self.u32()?;
        }
        if flags & ATTR_PERMISSIONS != 0 {
            self.u32()?;
        }
        if flags & ATTR_ACMODTIME != 0 {
            self.u32()?;
            self.u32()?;
        }
        if flags & ATTR_EXTENDED != 0 {
            let count = self.u32()?;
            for _ in 0..count {
                self.bytes()?;
                self.bytes()?;
            }
        }
        Ok(())
    }
}

/// Lit un paquet entier : (type, contenu après le type).
pub async fn read_packet<S: AsyncRead + Unpin>(stream: &mut S) -> Result<(u8, Vec<u8>)> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).await?;
    let len = u32::from_be_bytes(len) as usize;
    if len == 0 || len > MAX_PACKET {
        return Err(Error::Protocol(format!("packet of {len} bytes refused")));
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    let kind = body[0];
    body.remove(0);
    Ok((kind, body))
}

// --------------------------------------------------------------------------
// Client
// --------------------------------------------------------------------------

pub struct Sftp<S> {
    stream: S,
    next_id: u32,
}

impl<S: AsyncRead + AsyncWrite + Unpin> Sftp<S> {
    /// Négocie la version 3.
    pub async fn init(mut stream: S) -> Result<Self> {
        stream.write_all(&Packet::new(FXP_INIT).u32(3).framed()).await?;
        stream.flush().await?;
        let (kind, body) = read_packet(&mut stream).await?;
        if kind != FXP_VERSION {
            return Err(Error::Protocol(format!("unexpected answer {kind} to the handshake")));
        }
        let version = Reader::new(&body).u32()?;
        if version < 3 {
            return Err(Error::Protocol(format!("server speaks version {version}, 3 needed")));
        }
        Ok(Self { stream, next_id: 1 })
    }

    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    async fn send(&mut self, packet: Packet) -> Result<()> {
        self.stream.write_all(&packet.framed()).await?;
        Ok(())
    }

    /// Envoie une requête et attend sa réponse.
    async fn call(
        &mut self,
        kind: u8,
        fill: impl FnOnce(Packet) -> Packet,
    ) -> Result<(u8, Vec<u8>)> {
        let id = self.id();
        self.send(fill(Packet::new(kind).u32(id))).await?;
        self.stream.flush().await?;
        let (kind, body) = read_packet(&mut self.stream).await?;
        let answered = Reader::new(&body).u32()?;
        if answered != id {
            return Err(Error::Protocol(format!("answer to request {answered}, {id} expected")));
        }
        Ok((kind, body[4..].to_vec()))
    }

    async fn expect_ok(&mut self, kind: u8, fill: impl FnOnce(Packet) -> Packet) -> Result<()> {
        let (answer, body) = self.call(kind, fill).await?;
        status(answer, &body)
    }

    async fn expect_handle(
        &mut self,
        kind: u8,
        fill: impl FnOnce(Packet) -> Packet,
    ) -> Result<Vec<u8>> {
        let (answer, body) = self.call(kind, fill).await?;
        if answer == FXP_HANDLE {
            return Ok(Reader::new(&body).bytes()?.to_vec());
        }
        status(answer, &body)?;
        Err(Error::Protocol("no handle in the answer".into()))
    }

    /// Ouvre (crée, tronque) un fichier en écriture.
    pub async fn open_write(&mut self, path: &str) -> Result<Vec<u8>> {
        let flags = FXF_WRITE | FXF_CREAT | FXF_TRUNC;
        self.expect_handle(FXP_OPEN, |p| p.string(path).u32(flags).u32(0)).await
    }

    pub async fn close(&mut self, handle: &[u8]) -> Result<()> {
        self.expect_ok(FXP_CLOSE, |p| p.bytes(handle)).await
    }

    pub async fn remove(&mut self, path: &str) -> Result<()> {
        self.expect_ok(FXP_REMOVE, |p| p.string(path)).await
    }

    pub async fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        self.expect_ok(FXP_RENAME, |p| p.string(from).string(to)).await
    }

    /// Écrit tout `reader` dans `handle`, plusieurs requêtes en vol. Rend le
    /// nombre d'octets écrits.
    pub async fn write_from<R: AsyncRead + Unpin>(
        &mut self,
        handle: &[u8],
        reader: &mut R,
    ) -> Result<u64> {
        let mut buffer = vec![0u8; WRITE_CHUNK];
        let mut offset = 0u64;
        let mut pending: VecDeque<u32> = VecDeque::new();
        let mut eof = false;
        loop {
            while !eof && pending.len() < WINDOW {
                let read = reader.read(&mut buffer).await?;
                if read == 0 {
                    eof = true;
                    break;
                }
                let id = self.id();
                let data = &buffer[..read];
                let packet = Packet::new(FXP_WRITE).u32(id).bytes(handle).u64(offset).bytes(data);
                self.send(packet).await?;
                pending.push_back(id);
                offset += read as u64;
            }
            self.stream.flush().await?;
            if pending.is_empty() {
                return Ok(offset);
            }
            let (kind, body) = read_packet(&mut self.stream).await?;
            let id = Reader::new(&body).u32()?;
            let Some(position) = pending.iter().position(|waiting| *waiting == id) else {
                return Err(Error::Protocol(format!("answer to unknown request {id}")));
            };
            pending.remove(position);
            status(kind, &body[4..])?;
        }
    }

    /// Noms des entrées d'un dossier (`.` et `..` exclus).
    pub async fn list_dir(&mut self, path: &str) -> Result<Vec<String>> {
        let handle = self.expect_handle(FXP_OPENDIR, |p| p.string(path)).await?;
        let mut names = Vec::new();
        loop {
            let (kind, body) = self.call(FXP_READDIR, |p| p.bytes(&handle)).await?;
            if kind == FXP_NAME {
                let mut reader = Reader::new(&body);
                let count = reader.u32()?;
                for _ in 0..count {
                    let name = reader.string()?;
                    reader.bytes()?;
                    reader.skip_attrs()?;
                    if name != "." && name != ".." {
                        names.push(name);
                    }
                }
                continue;
            }
            if kind == FXP_STATUS && Reader::new(&body).u32()? == FX_EOF {
                break;
            }
            status(kind, &body)?;
            return Err(Error::Protocol("unexpected answer while listing".into()));
        }
        let _ = self.close(&handle).await;
        Ok(names)
    }
}

/// Un statut : rien pour `FX_OK`, une erreur sinon (ou une réponse d'un autre
/// type que celui attendu).
fn status(kind: u8, body: &[u8]) -> Result<()> {
    if kind != FXP_STATUS {
        return Err(Error::Protocol(format!("unexpected answer type {kind}")));
    }
    let mut reader = Reader::new(body);
    let code = reader.u32()?;
    if code == FX_OK {
        return Ok(());
    }
    let message = reader.string().unwrap_or_default();
    Err(Error::Status { code, message })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use super::*;

    type Files = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

    fn reply_status(id: u32, code: u32) -> Packet {
        Packet::new(FXP_STATUS).u32(id).u32(code).string("").string("")
    }

    /// Faux serveur SFTP en mémoire : juste les requêtes du client.
    async fn serve<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, files: Files) {
        let mut handles: BTreeMap<Vec<u8>, (String, bool)> = BTreeMap::new();
        let mut counter = 0u32;
        while let Ok((kind, body)) = read_packet(&mut stream).await {
            let mut reader = Reader::new(&body);
            if kind == FXP_INIT {
                let _ = stream.write_all(&Packet::new(FXP_VERSION).u32(3).framed()).await;
                continue;
            }
            let id = reader.u32().unwrap();
            let reply = match kind {
                FXP_OPEN => {
                    let path = reader.string().unwrap();
                    files.lock().unwrap().insert(path.clone(), Vec::new());
                    counter += 1;
                    let handle = format!("h{counter}").into_bytes();
                    handles.insert(handle.clone(), (path, false));
                    Packet::new(FXP_HANDLE).u32(id).bytes(&handle)
                }
                FXP_WRITE => {
                    let handle = reader.bytes().unwrap().to_vec();
                    let offset = reader.u64().unwrap() as usize;
                    let data = reader.bytes().unwrap();
                    let (path, _) = handles.get(&handle).unwrap().clone();
                    let mut files = files.lock().unwrap();
                    let file = files.get_mut(&path).unwrap();
                    if file.len() < offset + data.len() {
                        file.resize(offset + data.len(), 0);
                    }
                    file[offset..offset + data.len()].copy_from_slice(data);
                    reply_status(id, FX_OK)
                }
                FXP_CLOSE => reply_status(id, FX_OK),
                FXP_REMOVE => {
                    let path = reader.string().unwrap();
                    let gone = files.lock().unwrap().remove(&path).is_some();
                    reply_status(id, if gone { FX_OK } else { FX_NO_SUCH_FILE })
                }
                FXP_RENAME => {
                    let from = reader.string().unwrap();
                    let to = reader.string().unwrap();
                    let mut files = files.lock().unwrap();
                    match files.remove(&from) {
                        Some(data) => {
                            files.insert(to, data);
                            reply_status(id, FX_OK)
                        }
                        None => reply_status(id, FX_NO_SUCH_FILE),
                    }
                }
                FXP_OPENDIR => {
                    let path = reader.string().unwrap();
                    counter += 1;
                    let handle = format!("d{counter}").into_bytes();
                    handles.insert(handle.clone(), (path, false));
                    Packet::new(FXP_HANDLE).u32(id).bytes(&handle)
                }
                FXP_READDIR => {
                    let handle = reader.bytes().unwrap().to_vec();
                    let (dir, done) = handles.get(&handle).unwrap().clone();
                    if done {
                        reply_status(id, FX_EOF)
                    } else {
                        handles.insert(handle, (dir.clone(), true));
                        let files = files.lock().unwrap();
                        let prefix = format!("{dir}/");
                        let names: Vec<&str> = files
                            .keys()
                            .filter_map(|path| path.strip_prefix(&prefix))
                            .chain([".", ".."])
                            .collect();
                        let mut packet = Packet::new(FXP_NAME).u32(id).u32(names.len() as u32);
                        for name in names {
                            // ATTRS avec taille et droits, comme OpenSSH.
                            packet = packet
                                .string(name)
                                .string(&format!("-rw------- 1 u g 0 {name}"))
                                .u32(ATTR_SIZE | ATTR_PERMISSIONS)
                                .u64(0)
                                .u32(0o600);
                        }
                        packet
                    }
                }
                _ => reply_status(id, 8),
            };
            if stream.write_all(&reply.framed()).await.is_err() {
                break;
            }
        }
    }

    #[tokio::test]
    async fn a_file_is_written_renamed_listed_and_removed() {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let files: Files = Arc::default();
        tokio::spawn(serve(server, files.clone()));

        let mut sftp = Sftp::init(client).await.unwrap();
        // Plus que la fenêtre entière : les accusés arrivent pendant l'envoi.
        let data: Vec<u8> = (0..(WRITE_CHUNK * WINDOW * 2 + 123)).map(|i| i as u8).collect();
        let handle = sftp.open_write("backups/x.part").await.unwrap();
        let written = sftp.write_from(&handle, &mut data.as_slice()).await.unwrap();
        assert_eq!(written, data.len() as u64);
        sftp.close(&handle).await.unwrap();
        sftp.rename("backups/x.part", "backups/x.db").await.unwrap();

        assert_eq!(files.lock().unwrap().get("backups/x.db"), Some(&data));
        assert_eq!(sftp.list_dir("backups").await.unwrap(), vec!["x.db".to_string()]);

        sftp.remove("backups/x.db").await.unwrap();
        assert!(sftp.list_dir("backups").await.unwrap().is_empty());
        let missing = sftp.remove("backups/x.db").await.unwrap_err();
        assert!(matches!(missing, Error::Status { code: FX_NO_SUCH_FILE, .. }), "{missing}");
    }

    #[test]
    fn attributes_of_every_kind_are_skipped() {
        let packet = Packet::new(0)
            .u32(ATTR_SIZE | ATTR_UIDGID | ATTR_PERMISSIONS | ATTR_ACMODTIME | ATTR_EXTENDED)
            .u64(5)
            .u32(1)
            .u32(2)
            .u32(0o644)
            .u32(10)
            .u32(11)
            .u32(1)
            .string("k")
            .string("v")
            .string("after");
        let framed = packet.framed();
        let mut reader = Reader::new(&framed[5..]);
        reader.skip_attrs().unwrap();
        assert_eq!(reader.string().unwrap(), "after");
    }

    #[test]
    fn a_truncated_packet_is_an_error_not_a_panic() {
        let mut reader = Reader::new(&[0, 0, 0, 9, 1, 2]);
        assert!(reader.bytes().is_err());
    }
}
