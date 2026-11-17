//! Tampon de reprise : ce que l'agent garde sous le coude quand le serveur ne
//! répond pas.
//!
//! Sans lui, redémarrer le serveur creuserait un trou dans tous les graphes du
//! parc. Avec lui, les mesures partent avec leur horodatage d'origine dès que le
//! serveur revient, et les courbes se referment d'elles-mêmes.
//!
//! Deux étages :
//!
//! - **la mémoire**, en temps normal : un lot part à chaque cycle, rien n'est
//!   écrit sur le disque (une carte SD n'a pas à encaisser chaque mesure) ;
//! - **le disque** ([`Spool`]), dès qu'un envoi échoue : ce qui attendait en
//!   mémoire y passe, et chaque cycle suivant y est écrit au fil de l'eau
//!   jusqu'au retour du serveur. Une coupure de courant pendant la panne ne
//!   perd donc rien, et le rattrapage vide le disque, le plus ancien d'abord.
//!
//! Les deux sont bornés — la mémoire en échantillons, le disque en octets — et
//! sacrifient le plus ancien quand ils sont pleins : une panne longue ne doit
//! jamais finir par faire tuer l'agent ou remplir un disque — l'outil de
//! surveillance ne doit jamais devenir la panne.

use std::collections::VecDeque;
use std::path::Path;

use dumbmonit_proto::Sample;
use tracing::warn;

use crate::spool::{self, Spool};

/// Échantillons au plus par enregistrement du tampon disque.
const RECORD_SAMPLES: usize = 2_000;

/// Enregistrements relus au plus d'un coup sur le disque pour composer un lot.
const PEEK_RECORDS: usize = 64;
const PEEK_BYTES: usize = 8 * 1024 * 1024;

/// D'où vient le lot en cours d'envoi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Inflight {
    Memory,
    /// Du disque : les enregistrements entiers jusqu'à `seq` (`whole`), ou le
    /// début de l'enregistrement `seq`, jusqu'à l'échantillon `skip` exclu.
    Disk {
        seq: u64,
        whole: bool,
        skip: usize,
    },
}

pub struct PendingBuffer {
    samples: VecDeque<Sample>,
    capacity: usize,
    /// Compteur cumulé des pertes de la mémoire, remonté comme métrique (avec
    /// celles du disque) : une valeur non nulle dit noir sur blanc que les
    /// graphes de cette machine ont un trou.
    dropped: u64,
    /// Tampon disque ; `None` s'il est désactivé ou n'a pas pu s'ouvrir.
    spool: Option<Spool>,
    /// Vrai pendant une coupure : les mesures vont directement sur le disque.
    spilling: bool,
    inflight: Option<Inflight>,
    /// Échantillons déjà partis de l'enregistrement de tête du disque, quand un
    /// lot plus petit qu'un enregistrement l'a recoupé.
    head_skip: usize,
}

impl PendingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            capacity: capacity.max(1),
            dropped: 0,
            spool: None,
            spilling: false,
            inflight: None,
            head_skip: 0,
        }
    }

    /// Adjoint le tampon disque. Ce qu'il contient déjà (arrêt ou coupure
    /// pendant une panne) part en premier.
    pub fn with_spool(mut self, spool: Option<Spool>) -> Self {
        self.spool = spool;
        self
    }

    pub fn has_spool(&self) -> bool {
        self.spool.is_some()
    }

    /// Change la borne du tampon disque (partagée avec le relais).
    pub fn set_disk_limit(&mut self, bytes: u64) {
        if let Some(spool) = self.spool.as_mut()
            && let Err(error) = spool.set_limit(bytes)
        {
            warn!(%error, "cannot apply the catch-up buffer limit");
        }
    }

    /// Échantillons en attente, en mémoire et sur le disque.
    pub fn len(&self) -> usize {
        let disk = self.spool.as_ref().map_or(0, |s| s.pending_weight() as usize);
        self.samples.len() + disk.saturating_sub(self.head_skip)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty() && self.spool.as_ref().is_none_or(Spool::is_empty)
    }

    /// Vrai s'il reste du retard sur le disque : le rattrapage est en cours.
    pub fn catching_up(&self) -> bool {
        self.spool.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Pertes cumulées, mémoire et disque.
    pub fn dropped(&self) -> u64 {
        self.dropped.saturating_add(self.spool.as_ref().map_or(0, Spool::dropped))
    }

    /// Octets occupés sur le disque.
    pub fn disk_bytes(&self) -> u64 {
        self.spool.as_ref().map_or(0, Spool::bytes)
    }

    /// Ajoute un lot fraîchement collecté : sur le disque pendant une coupure,
    /// en mémoire sinon (ou si le disque refuse l'écriture).
    pub fn push(&mut self, samples: Vec<Sample>) {
        if samples.is_empty() {
            return;
        }
        if self.spilling && self.spool.is_some() {
            match self.write_disk(&samples) {
                Ok(()) => return,
                Err(error) => {
                    warn!(%error, "cannot write the catch-up buffer to disk, kept in memory")
                }
            }
        }
        self.samples.extend(samples);
        self.enforce_capacity();
    }

    /// Un envoi vient d'échouer : ce qui attend en mémoire passe sur le disque,
    /// et les mesures suivantes iront directement jusqu'à [`Self::resume`].
    pub fn spill(&mut self) {
        if self.spool.is_none() {
            return;
        }
        self.spilling = true;
        if self.samples.is_empty() || self.inflight.is_some() {
            return;
        }
        let samples: Vec<Sample> = self.samples.drain(..).collect();
        if let Err(error) = self.write_disk(&samples) {
            warn!(%error, "cannot write the catch-up buffer to disk, kept in memory");
            self.samples = samples.into();
        }
    }

    /// Le serveur répond de nouveau : les mesures fraîches restent en mémoire,
    /// le disque se vide au rythme des lots.
    pub fn resume(&mut self) {
        self.spilling = false;
    }

    fn write_disk(&mut self, samples: &[Sample]) -> std::io::Result<()> {
        let Some(spool) = self.spool.as_mut() else {
            return Err(std::io::Error::other("no disk buffer"));
        };
        let mut encoded = Vec::with_capacity(samples.len().div_ceil(RECORD_SAMPLES));
        for chunk in samples.chunks(RECORD_SAMPLES) {
            encoded.push((spool::encode(chunk)?, chunk.len() as u32));
        }
        let records: Vec<(&[u8], u32)> =
            encoded.iter().map(|(payload, weight)| (payload.as_slice(), *weight)).collect();
        spool.append_weighted(&records)
    }

    /// Prélève au plus `max` échantillons, les plus anciens d'abord : ceux du
    /// disque, puis ceux de la mémoire. Le prélèvement doit être suivi de
    /// [`Self::commit`] (envoyé, ou jeté pour de bon) ou de [`Self::restore`]
    /// (à renvoyer).
    ///
    /// L'ordre chronologique est préservé : VictoriaMetrics accepte les points
    /// dans le désordre, mais un rattrapage lisible aide énormément au diagnostic.
    pub fn take(&mut self, max: usize) -> Vec<Sample> {
        let max = max.max(1);
        self.inflight = None;
        if self.catching_up() {
            match self.take_disk(max) {
                Ok(Some(samples)) => return samples,
                Ok(None) => {}
                Err(error) => warn!(%error, "cannot read the catch-up buffer on disk"),
            }
        }
        let count = max.min(self.samples.len());
        if count > 0 {
            self.inflight = Some(Inflight::Memory);
        }
        self.samples.drain(..count).collect()
    }

    fn take_disk(&mut self, max: usize) -> std::io::Result<Option<Vec<Sample>>> {
        let Some(spool) = self.spool.as_mut() else { return Ok(None) };
        let records = spool.peek(PEEK_RECORDS, PEEK_BYTES)?;
        let mut out: Vec<Sample> = Vec::new();
        let mut last_whole: Option<u64> = None;
        for (index, record) in records.iter().enumerate() {
            let mut samples: Vec<Sample> = match spool::decode(&record.payload) {
                Ok(samples) => samples,
                Err(error) if out.is_empty() => {
                    // Illisible (octets abîmés que la somme de contrôle n'a pas
                    // vus, format inconnu) : jeté et compté.
                    warn!(%error, seq = record.seq, "unreadable catch-up record, dropped");
                    self.dropped = self.dropped.saturating_add(u64::from(record.weight));
                    if index == 0 {
                        self.head_skip = 0;
                    }
                    last_whole = Some(record.seq);
                    continue;
                }
                // Le suivant illisible attendra d'être en tête.
                Err(_) => break,
            };
            let skip = if index == 0 { self.head_skip.min(samples.len()) } else { 0 };
            let remaining = samples.len() - skip;
            if out.len() + remaining <= max {
                out.extend(samples.drain(skip..));
                last_whole = Some(record.seq);
                continue;
            }
            if out.is_empty() && last_whole.is_none() {
                // Un seul enregistrement dépasse déjà le lot (lots recoupés
                // après un 413) : on en envoie le début.
                let end = skip + max;
                out.extend(samples.drain(skip..end));
                self.inflight = Some(Inflight::Disk { seq: record.seq, whole: false, skip: end });
                return Ok(Some(out));
            }
            break;
        }
        let Some(seq) = last_whole else { return Ok(None) };
        if out.is_empty() {
            // Rien que de l'illisible, ou un reste vide : acquitté sans envoi.
            spool.ack(seq)?;
            self.head_skip = 0;
            return Ok(None);
        }
        self.inflight = Some(Inflight::Disk { seq, whole: true, skip: 0 });
        Ok(Some(out))
    }

    /// Le dernier prélèvement est parti (ou a été jeté pour de bon) : ce qui
    /// venait du disque y est acquitté.
    pub fn commit(&mut self) {
        match self.inflight.take() {
            Some(Inflight::Disk { seq, whole: true, .. }) => {
                self.head_skip = 0;
                if let Some(spool) = self.spool.as_mut()
                    && let Err(error) = spool.ack(seq)
                {
                    warn!(%error, "cannot acknowledge the catch-up buffer on disk");
                }
            }
            Some(Inflight::Disk { skip, whole: false, .. }) => self.head_skip = skip,
            Some(Inflight::Memory) | None => {}
        }
    }

    /// Remet un prélèvement dont l'envoi a échoué.
    ///
    /// Venu du disque, il y est resté : rien à faire. Venu de la mémoire, il
    /// est remis en tête ; si la remise fait déborder le tampon, ce sont encore
    /// les plus anciens qui partent : au moment de choisir, une mesure récente
    /// vaut mieux qu'une vieille.
    pub fn restore(&mut self, samples: Vec<Sample>) {
        if let Some(Inflight::Disk { .. }) = self.inflight.take() {
            return;
        }
        for sample in samples.into_iter().rev() {
            self.samples.push_front(sample);
        }
        self.enforce_capacity();
    }

    /// Écrit sur disque au plus `max` mesures, les plus récentes, pour le
    /// prochain démarrage. Rend le nombre de mesures écrites. Ne sert que sans
    /// tampon disque (désactivé, ou impossible à ouvrir).
    ///
    /// Écriture dans un fichier voisin puis renommage : un arrêt brutal pendant
    /// l'écriture laisse l'ancien fichier, ou aucun, jamais un fichier tronqué.
    pub fn save(&self, path: &Path, max: usize) -> std::io::Result<usize> {
        let skip = self.samples.len().saturating_sub(max);
        let kept: Vec<&Sample> = self.samples.iter().skip(skip).collect();
        let body = serde_json::to_vec(&kept).map_err(std::io::Error::other)?;
        let partial = path.with_extension("json.partial");
        std::fs::write(&partial, body)?;
        std::fs::rename(&partial, path)?;
        Ok(kept.len())
    }

    /// Relit les mesures gardées par un arrêt précédent et efface le fichier :
    /// elles repartent une fois, pas à chaque démarrage. Un fichier illisible
    /// est effacé lui aussi — le garder bloquerait la reprise à jamais.
    pub fn load(path: &Path) -> Vec<Sample> {
        let Ok(body) = std::fs::read(path) else { return Vec::new() };
        let _ = std::fs::remove_file(path);
        match serde_json::from_slice::<Vec<Sample>>(&body) {
            Ok(samples) => samples,
            Err(error) => {
                let path = path.display();
                tracing::warn!(%error, %path, "unreadable kept measurements, discarded");
                Vec::new()
            }
        }
    }

    fn enforce_capacity(&mut self) {
        if self.samples.len() <= self.capacity {
            return;
        }
        let excess = self.samples.len() - self.capacity;
        self.samples.drain(..excess);
        self.dropped = self.dropped.saturating_add(excess as u64);
    }
}

#[cfg(test)]
mod tests {
    use dumbmonit_proto::MetricKind;

    use super::*;

    fn sample(ts_ms: i64) -> Sample {
        Sample::new("cpu_usage_percent", 1.0, MetricKind::Gauge, ts_ms)
    }

    fn batch(range: std::ops::Range<i64>) -> Vec<Sample> {
        range.map(sample).collect()
    }

    fn stamps(samples: &[Sample]) -> Vec<i64> {
        samples.iter().map(|s| s.ts_ms).collect()
    }

    fn with_disk(dir: &Path, limit: u64) -> PendingBuffer {
        PendingBuffer::new(1_000).with_spool(Some(Spool::open(dir, limit).expect("tampon")))
    }

    /// Vide le tampon par lots de `max`, comme la boucle d'envoi.
    fn drain(buffer: &mut PendingBuffer, max: usize) -> Vec<i64> {
        let mut sent = Vec::new();
        loop {
            let lot = buffer.take(max);
            if lot.is_empty() {
                return sent;
            }
            sent.extend(stamps(&lot));
            buffer.commit();
        }
    }

    #[test]
    fn measurements_kept_at_shutdown_come_back_once_at_startup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pending-samples.json");
        let mut buffer = PendingBuffer::new(100);
        buffer.push(batch(0..10));
        // Borné : ce sont les plus récentes qui sont gardées.
        assert_eq!(buffer.save(&path, 4).unwrap(), 4);

        let back = PendingBuffer::load(&path);
        assert_eq!(back.iter().map(|s| s.ts_ms).collect::<Vec<_>>(), vec![6, 7, 8, 9]);
        assert!(!path.exists(), "relues une fois, pas à chaque démarrage");
        assert!(PendingBuffer::load(&path).is_empty());

        std::fs::write(&path, "pas du JSON").unwrap();
        assert!(PendingBuffer::load(&path).is_empty());
        assert!(!path.exists(), "un fichier illisible ne bloque pas les démarrages suivants");
    }

    #[test]
    fn samples_come_out_in_the_order_they_went_in() {
        let mut buffer = PendingBuffer::new(10);
        buffer.push(batch(0..3));
        buffer.push(batch(3..6));

        let taken = buffer.take(4);
        assert_eq!(taken.iter().map(|s| s.ts_ms).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn taking_more_than_available_returns_everything() {
        let mut buffer = PendingBuffer::new(10);
        buffer.push(batch(0..3));
        assert_eq!(buffer.take(100).len(), 3);
        assert!(buffer.is_empty());
    }

    #[test]
    fn the_oldest_samples_are_sacrificed_when_the_buffer_is_full() {
        let mut buffer = PendingBuffer::new(4);
        buffer.push(batch(0..3));
        buffer.push(batch(3..6));

        assert_eq!(buffer.len(), 4);
        assert_eq!(buffer.dropped(), 2);
        // Ce sont bien les plus récents qui restent.
        assert_eq!(buffer.take(4).iter().map(|s| s.ts_ms).collect::<Vec<_>>(), vec![2, 3, 4, 5]);
    }

    #[test]
    fn a_failed_send_is_put_back_at_the_front() {
        let mut buffer = PendingBuffer::new(10);
        buffer.push(batch(0..4));

        let taken = buffer.take(2);
        buffer.push(batch(4..6));
        buffer.restore(taken);

        assert_eq!(
            buffer.take(10).iter().map(|s| s.ts_ms).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5],
            "la chronologie doit survivre à un échec d'envoi"
        );
    }

    #[test]
    fn restoring_into_a_full_buffer_still_drops_the_oldest() {
        let mut buffer = PendingBuffer::new(4);
        let taken = {
            let mut source = PendingBuffer::new(10);
            source.push(batch(0..3));
            source.take(3)
        };
        buffer.push(batch(3..7));
        buffer.restore(taken);

        assert_eq!(buffer.len(), 4);
        assert_eq!(buffer.take(4).iter().map(|s| s.ts_ms).collect::<Vec<_>>(), vec![3, 4, 5, 6]);
    }

    #[test]
    fn a_very_long_outage_never_grows_beyond_the_bound() {
        let mut buffer = PendingBuffer::new(100);
        for cycle in 0..1_000 {
            buffer.push(batch(cycle * 50..cycle * 50 + 50));
            assert!(buffer.len() <= 100);
        }
        assert!(buffer.dropped() > 0, "une panne aussi longue doit se voir");
    }

    #[test]
    fn a_null_capacity_is_brought_back_to_one() {
        // Une capacité nulle rendrait le tampon inutile et ferait boucler à vide la
        // logique d'éviction : on la corrige plutôt que de refuser de démarrer.
        let mut buffer = PendingBuffer::new(0);
        buffer.push(batch(0..3));
        assert_eq!(buffer.len(), 1);
    }

    #[test]
    fn nothing_touches_the_disk_while_the_server_answers() {
        let dir = tempfile::tempdir().unwrap();
        let mut buffer = with_disk(dir.path(), 1 << 20);
        buffer.push(batch(0..10));
        assert_eq!(buffer.disk_bytes(), 0);
        assert_eq!(drain(&mut buffer, 100), (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn an_outage_goes_to_disk_as_it_happens_and_survives_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let mut buffer = with_disk(dir.path(), 1 << 20);
        buffer.push(batch(0..5));
        // Premier échec : la mémoire passe sur le disque…
        let lot = buffer.take(100);
        buffer.restore(lot);
        buffer.spill();
        assert!(buffer.disk_bytes() > 0);
        // … et chaque cycle suivant y est écrit aussitôt.
        buffer.push(batch(5..8));
        buffer.push(batch(8..10));
        assert_eq!(buffer.len(), 10);
        // Coupure de courant : rien n'a été rangé « à l'arrêt ».
        drop(buffer);

        let mut buffer = with_disk(dir.path(), 1 << 20);
        assert_eq!(buffer.len(), 10, "tout est encore là au démarrage suivant");
        buffer.resume();
        buffer.push(batch(10..12));
        // Le disque d'abord, les mesures fraîches ensuite : la chronologie tient.
        assert_eq!(drain(&mut buffer, 4), (0..12).collect::<Vec<_>>());
        assert!(buffer.is_empty());
        assert_eq!(buffer.disk_bytes(), 0, "le rattrapage fait, le disque est rendu");
    }

    #[test]
    fn a_failed_catch_up_batch_stays_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut buffer = with_disk(dir.path(), 1 << 20);
        buffer.spill();
        buffer.push(batch(0..6));
        let lot = buffer.take(4);
        assert_eq!(stamps(&lot), vec![0, 1, 2, 3]);
        buffer.restore(lot);
        assert_eq!(buffer.len(), 6);
        assert_eq!(drain(&mut buffer, 100), (0..6).collect::<Vec<_>>());
    }

    #[test]
    fn a_record_larger_than_the_batch_is_sent_in_slices() {
        let dir = tempfile::tempdir().unwrap();
        let mut buffer = with_disk(dir.path(), 1 << 20);
        buffer.spill();
        buffer.push(batch(0..2_500)); // deux enregistrements : 2 000 et 500
        let sent = drain(&mut buffer, 700);
        assert_eq!(sent, (0..2_500).collect::<Vec<_>>());
        assert!(buffer.is_empty());
    }

    #[test]
    fn a_full_disk_buffer_drops_the_oldest_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        // Limite minimale (64 Kio) : quelques centaines de cycles suffisent.
        let mut buffer = with_disk(dir.path(), 1);
        buffer.spill();
        for cycle in 0..400 {
            let lot: Vec<Sample> = (0..200)
                .map(|i| sample(cycle * 200 + i).with_label("série", format!("s{i}-{cycle}")))
                .collect();
            buffer.push(lot);
            assert!(buffer.disk_bytes() <= 2 * 64 * 1024, "{}", buffer.disk_bytes());
        }
        assert!(buffer.dropped() > 0);
        let first = buffer.take(1);
        assert!(first[0].ts_ms > 0, "les plus anciens partent en premier");
    }
}
