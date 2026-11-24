-- Index du rapport de bruit : les déclenchements d'une fenêtre, la résolution
-- qui suit chacun d'eux, et le dernier déclenchement d'une empreinte (que
-- l'acquittement et le décompte des notifications mettent à jour).
CREATE INDEX IF NOT EXISTS idx_alert_history_phase_at ON alert_history(to_phase, at);
CREATE INDEX IF NOT EXISTS idx_alert_history_fp_phase ON alert_history(fingerprint, to_phase, at);
