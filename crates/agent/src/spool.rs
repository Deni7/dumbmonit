//! Tampon disque tournant : ce que l'agent met de côté quand le serveur ne
//! répond pas, et qui doit survivre à un redémarrage comme à une coupure de
//! courant.
//!
//! Un journal en ajout seul, découpé en segments (`<séquence>.seg`). Chaque
//! enregistrement porte un numéro de séquence croissant et une somme de
//! contrôle :
//!
//! ```text
//! [0xD7] [longueur u32 LE] [CRC-32 u32 LE] [séquence u64 LE] [poids u32 LE] [charge utile]
//! ```
//!
//! Le poids est ce que l'appelant veut compter (des échantillons, des résultats
//! de sonde) : le tampon en tient le total en attente sans relire les charges.
//!
//! - **Écrit au fil de l'eau** : chaque ajout est écrit puis forcé sur le disque
//!   (`fsync`) avant de rendre la main ; une coupure de courant n'emporte au plus
//!   que l'ajout en cours.
//! - **Reprise** : au démarrage, chaque segment est relu ; un enregistrement
//!   tronqué ou abîmé (écriture interrompue, zéros laissés par le système de
//!   fichiers) arrête la lecture du segment, qui est coupé à cet endroit. Le
//!   reste est rendu tel quel.
//! - **Borné en octets** : au-delà de la limite, le segment le plus ancien est
//!   effacé, et ses enregistrements non encore acquittés comptés comme perdus.
//! - **Acquittement** : le lecteur prélève les plus anciens (`peek`), les
//!   envoie, puis les acquitte (`ack`). La position acquittée est rangée dans
//!   `cursor.json` (écriture dans un fichier voisin puis renommage) ; un
//!   arrêt entre l'envoi et l'acquittement fait renvoyer le lot, jamais le
//!   perdre — au destinataire d'écarter le doublon.
//!
//! Le tampon vide ne pèse rien : les segments entièrement acquittés sont
//! effacés, il ne reste que le curseur.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Premier octet de chaque enregistrement.
const MAGIC: u8 = 0xD7;

/// Taille de l'en-tête : marque, longueur, somme de contrôle, séquence, poids.
const HEADER: usize = 1 + 4 + 4 + 8 + 4;

/// Charge utile maximale d'un enregistrement. Au-delà, une longueur lue sur le
/// disque est forcément fausse : on ne réserve pas la mémoire qu'elle annonce.
pub const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

/// Bornes de la taille d'un segment : un seizième de la limite, entre les deux.
/// Effacer le plus ancien libère ainsi de la place par tranches raisonnables.
const MIN_SEGMENT_BYTES: u64 = 64 * 1024;
const MAX_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;

const SEGMENT_EXTENSION: &str = "seg";
const CURSOR_FILE: &str = "cursor.json";

/// Ce qui est rangé dans `cursor.json`.
#[derive(Debug, Serialize, Deserialize)]
struct Cursor {
    /// Identité du tampon, tirée au hasard à sa création.
    id: String,
    /// Plus grande séquence acquittée.
    acked: u64,
    /// Poids perdu depuis la création (cumul).
    #[serde(default)]
    dropped: u64,
}

/// Un segment sur le disque.
#[derive(Debug)]
struct Segment {
    /// Séquence qui donne son nom au fichier.
    name: u64,
    /// Séquences du premier et du dernier enregistrement (`records > 0`).
    first: u64,
    last: u64,
    records: u64,
    bytes: u64,
    /// Début du premier enregistrement qui n'est pas encore connu comme
    /// acquitté : les lectures suivantes repartent de là.
    read_offset: u64,
    /// Poids de chaque enregistrement, de `first` à `last` (séquences
    /// contiguës dans un segment).
    weights: Vec<u32>,
}

impl Segment {
    /// Enregistrements de ce segment qui ne sont pas encore acquittés.
    fn pending(&self, acked: u64) -> u64 {
        if self.records == 0 || self.last <= acked {
            return 0;
        }
        let from = self.first.max(acked + 1);
        self.last - from + 1
    }

    /// Poids total des enregistrements non acquittés.
    fn pending_weight(&self, acked: u64) -> u64 {
        if self.records == 0 || self.last <= acked {
            return 0;
        }
        let skip = (self.first.max(acked + 1) - self.first) as usize;
        self.weights.iter().skip(skip).map(|w| u64::from(*w)).sum()
    }
}

/// Un enregistrement relu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub seq: u64,
    pub weight: u32,
    pub payload: Vec<u8>,
}

/// Tampon disque tournant, borné en octets.
pub struct Spool {
    dir: PathBuf,
    limit: u64,
    id: String,
    acked: u64,
    dropped: u64,
    next_seq: u64,
    segments: VecDeque<Segment>,
    /// Descripteur d'ajout du dernier segment, ouvert à la demande.
    writer: Option<File>,
    /// Ce que le dernier `peek` a rendu : séquence, segment, fin de
    /// l'enregistrement. L'acquittement s'en sert pour avancer la lecture.
    peeked: Vec<(u64, u64, u64)>,
}

impl Spool {
    /// Ouvre (ou crée) le tampon rangé dans `dir`, et relit ce qu'il contient.
    pub fn open(dir: &Path, limit_bytes: u64) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let cursor_path = dir.join(CURSOR_FILE);
        let (cursor, fresh) = match fs::read(&cursor_path) {
            Ok(body) => match serde_json::from_slice::<Cursor>(&body) {
                Ok(cursor) => (cursor, false),
                Err(error) => {
                    // Mieux vaut renvoyer ce qui l'a déjà été que de perdre ce
                    // qui ne l'a pas été : nouvelle identité, tout repart.
                    warn!(path = %cursor_path.display(), %error, "unreadable spool cursor, resending everything");
                    (Cursor { id: new_id(), acked: 0, dropped: 0 }, true)
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (Cursor { id: new_id(), acked: 0, dropped: 0 }, true)
            }
            Err(error) => return Err(error),
        };

        let mut spool = Self {
            dir: dir.to_path_buf(),
            limit: limit_bytes.max(MIN_SEGMENT_BYTES),
            id: cursor.id,
            acked: cursor.acked,
            dropped: cursor.dropped,
            next_seq: 1,
            segments: VecDeque::new(),
            writer: None,
            peeked: Vec::new(),
        };

        let mut names: Vec<u64> = fs::read_dir(dir)?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some(SEGMENT_EXTENSION) {
                    return None;
                }
                path.file_stem()?.to_str()?.parse::<u64>().ok()
            })
            .collect();
        names.sort_unstable();

        let mut damaged = false;
        for name in names {
            let path = spool.segment_path(name);
            let found = scan(&path)?;
            if found.torn {
                damaged = true;
                spool.dropped = spool.dropped.saturating_add(1);
                warn!(
                    path = %path.display(),
                    kept = found.records,
                    "damaged spool segment (interrupted write?), cut after its last good record"
                );
                OpenOptions::new().write(true).open(&path)?.set_len(found.good_bytes)?;
            }
            let segment = Segment {
                name,
                first: found.first,
                last: found.last,
                records: found.records,
                bytes: found.good_bytes,
                read_offset: 0,
                weights: found.weights,
            };
            if segment.records == 0 || segment.last <= spool.acked {
                let _ = fs::remove_file(&path);
                continue;
            }
            spool.segments.push_back(segment);
        }
        let last = spool.segments.back().map_or(0, |s| s.last);
        spool.next_seq = last.max(spool.acked) + 1;
        if fresh || damaged {
            spool.save_cursor()?;
        }
        if spool.pending() > 0 {
            debug!(dir = %dir.display(), pending = spool.pending(), "spool reopened");
        }
        Ok(spool)
    }

    /// Identité du tampon (voir [`dumbmonit_proto::RelayBacklog::spool_id`]).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Enregistrements en attente d'acquittement.
    pub fn pending(&self) -> u64 {
        self.segments.iter().map(|s| s.pending(self.acked)).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.pending() == 0
    }

    /// Poids total de ce qui attend (voir [`Spool::append_weighted`]).
    pub fn pending_weight(&self) -> u64 {
        self.segments.iter().map(|s| s.pending_weight(self.acked)).sum()
    }

    /// Octets occupés sur le disque, segments seulement.
    pub fn bytes(&self) -> u64 {
        self.segments.iter().map(|s| s.bytes).sum()
    }

    /// Poids perdu depuis la création du tampon : enregistrements effacés
    /// faute de place, abîmés, ou trop gros. Un enregistrement abîmé dont le
    /// poids est illisible compte pour un.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Change la limite, et l'applique aussitôt.
    pub fn set_limit(&mut self, limit_bytes: u64) -> io::Result<()> {
        self.limit = limit_bytes.max(MIN_SEGMENT_BYTES);
        if self.enforce_limit() {
            self.save_cursor()?;
        }
        Ok(())
    }

    /// Ajoute des enregistrements de poids 1 (voir [`Spool::append_weighted`]).
    #[cfg(test)]
    pub fn append(&mut self, payloads: &[Vec<u8>]) -> io::Result<()> {
        let records: Vec<(&[u8], u32)> = payloads.iter().map(|p| (p.as_slice(), 1)).collect();
        self.append_weighted(&records)
    }

    /// Ajoute des enregistrements `(charge utile, poids)`, dans l'ordre, et les
    /// force sur le disque. Une charge utile démesurée est écartée et comptée
    /// comme perdue.
    pub fn append_weighted(&mut self, records: &[(&[u8], u32)]) -> io::Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let segment_limit = self.segment_limit();
        let dropped_before = self.dropped;
        for &(payload, weight) in records {
            if payload.len() > MAX_RECORD_BYTES {
                self.dropped = self.dropped.saturating_add(u64::from(weight));
                warn!(bytes = payload.len(), "record too large for the spool, dropped");
                continue;
            }
            if self.needs_rotation(segment_limit) {
                self.start_segment()?;
            } else if self.writer.is_none() {
                let name = self.segments.back().map_or(self.next_seq, |s| s.name);
                self.writer =
                    Some(OpenOptions::new().append(true).open(self.segment_path(name))?);
            }
            let seq = self.next_seq;
            let record = frame(seq, weight, payload);
            let writer = self.writer.as_mut().expect("descripteur ouvert juste au-dessus");
            if let Err(error) = writer.write_all(&record) {
                // Disque plein, le plus souvent : on retire l'enregistrement
                // partiel, pour que les suivants restent lisibles, et le
                // prochain ajout ouvrira un segment neuf.
                let good = self.segments.back().map_or(0, |s| s.bytes);
                let _ = writer.set_len(good);
                self.writer = None;
                return Err(error);
            }
            let segment = self.segments.back_mut().expect("segment ouvert juste au-dessus");
            if segment.records == 0 {
                segment.first = seq;
            }
            segment.last = seq;
            segment.records += 1;
            segment.bytes += record.len() as u64;
            segment.weights.push(weight);
            self.next_seq += 1;
        }
        if let Some(writer) = &self.writer {
            writer.sync_data()?;
        }
        if self.enforce_limit() || self.dropped != dropped_before {
            self.save_cursor()?;
        }
        Ok(())
    }

    /// Les plus anciens enregistrements non acquittés : au plus `max_records`,
    /// et pas au-delà de `max_bytes` de charge utile (un au moins, quelle que
    /// soit sa taille). Rien n'est retiré tant que [`Spool::ack`] ne l'a pas dit.
    pub fn peek(&mut self, max_records: usize, max_bytes: usize) -> io::Result<Vec<Record>> {
        self.peeked.clear();
        let mut out = Vec::new();
        let mut total = 0usize;
        let mut index = 0;
        while index < self.segments.len() {
            if out.len() >= max_records.max(1) || (total >= max_bytes && !out.is_empty()) {
                break;
            }
            let (name, offset, pending) = {
                let s = &self.segments[index];
                (s.name, s.read_offset, s.pending(self.acked))
            };
            if pending == 0 {
                index += 1;
                continue;
            }
            let path = self.segment_path(name);
            let mut reader = BufReader::new(File::open(&path)?);
            reader.seek(SeekFrom::Start(offset))?;
            let mut position = offset;
            let mut damaged = false;
            loop {
                if out.len() >= max_records.max(1) || (total >= max_bytes && !out.is_empty()) {
                    break;
                }
                match read_record(&mut reader)? {
                    Found::End => break,
                    Found::Damaged => {
                        damaged = true;
                        break;
                    }
                    Found::Record(record) => {
                        let end = position + (HEADER + record.payload.len()) as u64;
                        position = end;
                        if record.seq <= self.acked {
                            self.segments[index].read_offset = end;
                            continue;
                        }
                        total += record.payload.len();
                        self.peeked.push((record.seq, name, end));
                        out.push(record);
                    }
                }
            }
            if damaged {
                // Abîmé en cours de route (disque défaillant) : ce qui suit est
                // illisible et compté comme perdu ; le segment est coupé là.
                let segment = &mut self.segments[index];
                let mine: Vec<(u64, u32)> = out
                    .iter()
                    .filter(|r| r.seq >= segment.first)
                    .map(|r| (r.seq, r.weight))
                    .collect();
                let readable: u64 = mine.iter().map(|(_, weight)| u64::from(*weight)).sum();
                let lost = segment.pending_weight(self.acked).saturating_sub(readable);
                self.dropped = self.dropped.saturating_add(lost.max(1));
                warn!(path = %path.display(), lost, "damaged spool segment, unreadable records dropped");
                let good = position;
                segment.bytes = good;
                match mine.last() {
                    Some((last, _)) => {
                        segment.last = *last;
                        segment.records = segment.last - segment.first + 1;
                        segment.weights.truncate(segment.records as usize);
                    }
                    // Plus rien d'utile dedans : il sera effacé à l'acquittement.
                    None => {
                        segment.records = 0;
                        segment.weights.clear();
                    }
                }
                if let Ok(file) = OpenOptions::new().write(true).open(&path) {
                    let _ = file.set_len(good);
                }
                if index + 1 == self.segments.len() {
                    self.writer = None;
                }
                self.save_cursor()?;
            }
            index += 1;
        }
        Ok(out)
    }

    /// Acquitte tout ce qui va jusqu'à `seq` inclus : les segments entièrement
    /// acquittés sont effacés.
    pub fn ack(&mut self, seq: u64) -> io::Result<()> {
        let seq = seq.min(self.next_seq.saturating_sub(1));
        if seq <= self.acked {
            return Ok(());
        }
        self.acked = seq;
        for (peeked, name, end) in &self.peeked {
            if *peeked > seq {
                continue;
            }
            if let Some(segment) = self.segments.iter_mut().find(|s| s.name == *name) {
                segment.read_offset = segment.read_offset.max(*end);
            }
        }
        self.peeked.retain(|(peeked, _, _)| *peeked > seq);
        while self.segments.front().is_some_and(|s| s.pending(self.acked) == 0) {
            let segment = self.segments.pop_front().expect("vérifié juste au-dessus");
            if self.segments.is_empty() {
                self.writer = None;
            }
            remove(&self.segment_path(segment.name));
        }
        self.save_cursor()
    }

    /// Oublie tout ce qui attend (relais coupé, serveur qui n'en veut pas).
    /// Ce n'est pas une perte subie : le compteur de pertes n'en dit rien.
    pub fn clear(&mut self) -> io::Result<()> {
        self.writer = None;
        self.peeked.clear();
        for segment in self.segments.drain(..) {
            remove(&self.dir.join(format!("{:020}.{SEGMENT_EXTENSION}", segment.name)));
        }
        self.acked = self.next_seq.saturating_sub(1);
        self.save_cursor()
    }

    /// Vrai s'il faut ouvrir un segment neuf : aucun, le dernier plein, ou un
    /// dernier dont la numérotation ne se poursuivrait pas d'un seul pas (les
    /// séquences d'un segment se suivent, la relecture y veille).
    fn needs_rotation(&self, segment_limit: u64) -> bool {
        let Some(last) = self.segments.back() else { return true };
        last.bytes >= segment_limit
            || (last.records == 0 && last.bytes > 0)
            || (last.records > 0 && last.last + 1 != self.next_seq)
    }

    fn segment_limit(&self) -> u64 {
        (self.limit / 16).clamp(MIN_SEGMENT_BYTES, MAX_SEGMENT_BYTES)
    }

    fn segment_path(&self, name: u64) -> PathBuf {
        self.dir.join(format!("{name:020}.{SEGMENT_EXTENSION}"))
    }

    /// Ouvre un segment neuf, nommé d'après la prochaine séquence.
    fn start_segment(&mut self) -> io::Result<()> {
        if let Some(writer) = self.writer.take() {
            writer.sync_data()?;
        }
        let name = self.next_seq;
        let path = self.segment_path(name);
        // `truncate` : un fichier de ce nom ne peut être qu'un reste vide.
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(&path)?;
        drop(file);
        self.writer = Some(OpenOptions::new().append(true).open(&path)?);
        self.segments.push_back(Segment {
            name,
            first: name,
            last: name,
            records: 0,
            bytes: 0,
            read_offset: 0,
            weights: Vec::new(),
        });
        Ok(())
    }

    /// Efface les plus anciens segments tant que la limite est dépassée — le
    /// dernier, celui qu'on remplit, reste. Vrai si quelque chose est parti.
    fn enforce_limit(&mut self) -> bool {
        let mut removed = false;
        while self.segments.len() > 1 && self.bytes() > self.limit {
            let segment = self.segments.pop_front().expect("deux segments au moins");
            let lost = segment.pending_weight(self.acked);
            self.dropped = self.dropped.saturating_add(lost);
            self.peeked.retain(|(_, name, _)| *name != segment.name);
            remove(&self.segment_path(segment.name));
            if lost > 0 {
                debug!(lost, "spool full: oldest records dropped");
            }
            removed = true;
        }
        removed
    }

    /// Range le curseur : fichier voisin, forcé sur le disque, puis renommé.
    fn save_cursor(&self) -> io::Result<()> {
        let cursor = Cursor { id: self.id.clone(), acked: self.acked, dropped: self.dropped };
        let body = serde_json::to_vec(&cursor).map_err(io::Error::other)?;
        let path = self.dir.join(CURSOR_FILE);
        let partial = self.dir.join(format!("{CURSOR_FILE}.partial"));
        {
            let mut file =
                OpenOptions::new().create(true).write(true).truncate(true).open(&partial)?;
            file.write_all(&body)?;
            file.sync_all()?;
        }
        fs::rename(&partial, &path)
    }
}

/// Résultat de la relecture d'un segment.
struct Scan {
    first: u64,
    last: u64,
    records: u64,
    weights: Vec<u32>,
    /// Fin du dernier enregistrement valide.
    good_bytes: u64,
    /// Des octets suivent le dernier enregistrement valide.
    torn: bool,
}

fn scan(path: &Path) -> io::Result<Scan> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let mut scan =
        Scan { first: 0, last: 0, records: 0, weights: Vec::new(), good_bytes: 0, torn: false };
    loop {
        match read_record(&mut reader)? {
            Found::End => break,
            Found::Damaged => {
                scan.torn = true;
                break;
            }
            Found::Record(record) => {
                // Les séquences se suivent dans un segment : un saut ne peut
                // venir que d'octets abîmés qui ont eu de la chance.
                if scan.records > 0 && record.seq != scan.last + 1 {
                    scan.torn = true;
                    break;
                }
                if scan.records == 0 {
                    scan.first = record.seq;
                }
                scan.last = record.seq;
                scan.records += 1;
                scan.weights.push(record.weight);
                scan.good_bytes += (HEADER + record.payload.len()) as u64;
            }
        }
    }
    if scan.good_bytes < size {
        scan.torn = true;
    }
    Ok(scan)
}

/// Ce qu'on trouve à la position courante d'un segment.
enum Found {
    Record(Record),
    /// Fin du fichier, à une frontière d'enregistrement.
    End,
    /// Enregistrement tronqué ou abîmé.
    Damaged,
}

fn read_record(reader: &mut impl Read) -> io::Result<Found> {
    let mut header = [0u8; HEADER];
    let mut filled = 0;
    while filled < HEADER {
        match reader.read(&mut header[filled..]) {
            Ok(0) => return Ok(if filled == 0 { Found::End } else { Found::Damaged }),
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    if header[0] != MAGIC {
        return Ok(Found::Damaged);
    }
    let length = u32::from_le_bytes(header[1..5].try_into().expect("4 octets")) as usize;
    let crc = u32::from_le_bytes(header[5..9].try_into().expect("4 octets"));
    let seq = u64::from_le_bytes(header[9..17].try_into().expect("8 octets"));
    let weight = u32::from_le_bytes(header[17..21].try_into().expect("4 octets"));
    if length > MAX_RECORD_BYTES {
        return Ok(Found::Damaged);
    }
    let mut payload = vec![0u8; length];
    if let Err(error) = reader.read_exact(&mut payload) {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(Found::Damaged);
        }
        return Err(error);
    }
    if checksum(seq, weight, &payload) != crc {
        return Ok(Found::Damaged);
    }
    Ok(Found::Record(Record { seq, weight, payload }))
}

fn frame(seq: u64, weight: u32, payload: &[u8]) -> Vec<u8> {
    let mut record = Vec::with_capacity(HEADER + payload.len());
    record.push(MAGIC);
    record.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    record.extend_from_slice(&checksum(seq, weight, payload).to_le_bytes());
    record.extend_from_slice(&seq.to_le_bytes());
    record.extend_from_slice(&weight.to_le_bytes());
    record.extend_from_slice(payload);
    record
}

/// CRC-32 de la séquence, du poids et de la charge utile.
fn checksum(seq: u64, weight: u32, payload: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&seq.to_le_bytes());
    hasher.update(&weight.to_le_bytes());
    hasher.update(payload);
    hasher.finalize()
}

fn remove(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => warn!(path = %path.display(), %error, "cannot remove a spool segment"),
    }
}

/// Identité aléatoire d'un tampon neuf : 64 bits tirés des clés aléatoires que
/// la bibliothèque standard demande au système pour ses tables de hachage.
fn new_id() -> String {
    use std::hash::BuildHasher;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let seed = (nanos, std::process::id());
    format!("{:016x}", std::collections::hash_map::RandomState::new().hash_one(seed))
}

/// Charge utile compacte : JSON compressé (zlib, niveau rapide). Des mesures
/// se compressent d'un facteur dix environ : le tampon tient d'autant plus.
pub fn encode<T: Serialize + ?Sized>(value: &T) -> io::Result<Vec<u8>> {
    let json = serde_json::to_vec(value).map_err(io::Error::other)?;
    let mut encoder = flate2::write::ZlibEncoder::new(
        Vec::with_capacity(json.len() / 4),
        flate2::Compression::fast(),
    );
    encoder.write_all(&json)?;
    encoder.finish()
}

/// Relit une charge utile écrite par [`encode`].
pub fn decode<T: serde::de::DeserializeOwned>(payload: &[u8]) -> io::Result<T> {
    let mut json = Vec::new();
    flate2::read::ZlibDecoder::new(payload).read_to_end(&mut json)?;
    serde_json::from_slice(&json).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(n: u64) -> Vec<u8> {
        format!("enregistrement-{n}").into_bytes()
    }

    fn seqs(records: &[Record]) -> Vec<u64> {
        records.iter().map(|r| r.seq).collect()
    }

    fn segment_files(dir: &Path) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(SEGMENT_EXTENSION))
            .collect();
        files.sort();
        files
    }

    #[test]
    fn records_come_back_in_order_and_survive_a_restart_until_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1), payload(2), payload(3)]).unwrap();
        assert_eq!(spool.pending(), 3);

        let first = spool.peek(2, usize::MAX).unwrap();
        assert_eq!(seqs(&first), vec![1, 2]);
        assert_eq!(first[0].payload, payload(1));
        // Rien n'est retiré par une simple lecture.
        assert_eq!(spool.pending(), 3);
        spool.ack(2).unwrap();
        assert_eq!(spool.pending(), 1);
        let id = spool.id().to_string();
        drop(spool);

        // Redémarrage : seul le non-acquitté revient, sous la même identité,
        // et la numérotation continue.
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        assert_eq!(spool.id(), id);
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![3]);
        spool.append(&[payload(4)]).unwrap();
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![3, 4]);
    }

    #[test]
    fn an_unacknowledged_batch_is_sent_again_after_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1), payload(2)]).unwrap();
        // Lu, envoyé peut-être, mais l'agent est tué avant l'accusé.
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![1, 2]);
        drop(spool);
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![1, 2], "au moins une fois");
    }

    #[test]
    fn the_byte_bound_drops_the_oldest_first_and_counts_the_loss() {
        let dir = tempfile::tempdir().unwrap();
        // Limite minimale : segments de 64 Kio, limite de 64 Kio.
        let mut spool = Spool::open(dir.path(), MIN_SEGMENT_BYTES).unwrap();
        let big = vec![7u8; 10 * 1024];
        for _ in 0..40 {
            spool.append(std::slice::from_ref(&big)).unwrap();
            // Jamais plus que la limite, plus le segment en cours.
            assert!(spool.bytes() <= MIN_SEGMENT_BYTES * 2, "{}", spool.bytes());
        }
        assert!(spool.dropped() > 0, "une longue panne doit se voir");
        let oldest = spool.peek(1, usize::MAX).unwrap();
        assert!(oldest[0].seq > 1, "les plus anciens partent en premier");
        assert_eq!(spool.pending() + spool.dropped(), 40, "rien ne disparaît sans être compté");

        // Le compteur de pertes survit au redémarrage.
        let dropped = spool.dropped();
        drop(spool);
        let spool = Spool::open(dir.path(), MIN_SEGMENT_BYTES).unwrap();
        assert_eq!(spool.dropped(), dropped);
    }

    #[test]
    fn a_torn_write_is_cut_and_everything_before_it_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1), payload(2), payload(3)]).unwrap();
        drop(spool);

        // Coupure de courant pendant un ajout : un en-tête à moitié écrit, puis
        // les zéros qu'un système de fichiers laisse parfois derrière lui.
        let file = segment_files(dir.path()).pop().expect("segment");
        let good = fs::metadata(&file).unwrap().len();
        let mut handle = OpenOptions::new().append(true).open(&file).unwrap();
        handle.write_all(&frame(4, 1, &payload(4))[..9]).unwrap();
        handle.write_all(&[0u8; 32]).unwrap();
        drop(handle);

        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        assert_eq!(fs::metadata(&file).unwrap().len(), good, "segment coupé net");
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![1, 2, 3]);
        assert_eq!(spool.dropped(), 1);
        // Les ajouts suivants restent lisibles derrière la coupure.
        spool.append(&[payload(4)]).unwrap();
        drop(spool);
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        let records = spool.peek(10, usize::MAX).unwrap();
        assert_eq!(seqs(&records), vec![1, 2, 3, 4]);
        assert_eq!(records[3].payload, payload(4));
    }

    #[test]
    fn a_flipped_byte_is_caught_by_the_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1), payload(2)]).unwrap();
        drop(spool);
        let file = segment_files(dir.path()).pop().expect("segment");
        let mut bytes = fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        fs::write(&file, bytes).unwrap();

        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![1]);
    }

    #[test]
    fn an_empty_spool_leaves_no_segment_behind() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1), payload(2)]).unwrap();
        let records = spool.peek(10, usize::MAX).unwrap();
        spool.ack(records.last().unwrap().seq).unwrap();
        assert!(spool.is_empty());
        assert!(segment_files(dir.path()).is_empty(), "le tampon vide ne pèse rien");
        // La numérotation ne repart pas de 1 : le destinataire écarterait tout.
        spool.append(&[payload(3)]).unwrap();
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![3]);
    }

    #[test]
    fn an_unreadable_cursor_resends_everything_under_a_new_identity() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1), payload(2)]).unwrap();
        spool.ack(1).unwrap();
        let id = spool.id().to_string();
        drop(spool);
        fs::write(dir.path().join(CURSOR_FILE), "pas du JSON").unwrap();

        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        assert_ne!(spool.id(), id);
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![1, 2]);
    }

    #[test]
    fn the_pending_weight_follows_appends_acks_and_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append_weighted(&[(&b"a"[..], 10), (&b"b"[..], 20), (&b"c"[..], 30)]).unwrap();
        assert_eq!(spool.pending_weight(), 60);
        let records = spool.peek(1, usize::MAX).unwrap();
        assert_eq!(records[0].weight, 10);
        spool.ack(records[0].seq).unwrap();
        assert_eq!(spool.pending_weight(), 50);
        drop(spool);
        assert_eq!(Spool::open(dir.path(), 1 << 20).unwrap().pending_weight(), 50);
    }

    #[test]
    fn peeking_stops_at_the_byte_budget_but_always_returns_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[vec![1u8; 100], vec![2u8; 100], vec![3u8; 100]]).unwrap();
        assert_eq!(spool.peek(10, 150).unwrap().len(), 2);
        assert_eq!(spool.peek(10, 1).unwrap().len(), 1);
    }

    #[test]
    fn records_span_several_segments_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 16 << 20).unwrap();
        // Segments de 1 Mio : le quatrième enregistrement de 400 Kio ouvre le
        // second segment.
        let big = vec![9u8; 400 * 1024];
        for _ in 0..4 {
            spool.append(std::slice::from_ref(&big)).unwrap();
        }
        assert_eq!(segment_files(dir.path()).len(), 2);
        let records = spool.peek(10, usize::MAX).unwrap();
        assert_eq!(seqs(&records), vec![1, 2, 3, 4]);
        spool.ack(3).unwrap();
        assert_eq!(segment_files(dir.path()).len(), 1, "le segment acquitté est effacé");
        assert_eq!(seqs(&spool.peek(10, usize::MAX).unwrap()), vec![4]);
    }

    #[test]
    fn clearing_forgets_without_counting_a_loss() {
        let dir = tempfile::tempdir().unwrap();
        let mut spool = Spool::open(dir.path(), 1 << 20).unwrap();
        spool.append(&[payload(1)]).unwrap();
        spool.clear().unwrap();
        assert!(spool.is_empty());
        assert_eq!(spool.dropped(), 0);
        drop(spool);
        assert!(Spool::open(dir.path(), 1 << 20).unwrap().is_empty());
    }

    #[test]
    fn payloads_round_trip_compressed() {
        let values: Vec<u64> = (0..1_000).collect();
        let encoded = encode(&values).unwrap();
        assert!(encoded.len() < serde_json::to_vec(&values).unwrap().len());
        assert_eq!(decode::<Vec<u64>>(&encoded).unwrap(), values);
        assert!(decode::<Vec<u64>>(b"n'importe quoi").is_err());
    }
}
