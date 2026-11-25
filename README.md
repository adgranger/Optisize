# Optisize

Analyseur d'espace disque pour Windows dans l'esprit de TreeSize Free, écrit en Rust (egui/eframe). Interface en français.

- Scan parallèle en arrière-plan, l'arborescence se remplit en direct (le thread UI ne bloque jamais).
- Arbre compact (arène `Vec<Node>` + pool de noms), tableau virtualisé triable, treemap imbriqué, répartition par type, lecteurs.
- Pause / Reprendre / Arrêter / Rafraîchir. Les dossiers inaccessibles sont ignorés.

## Lancer

```powershell
cargo run --release              # ouvre l'application
cargo run --release -- C:\Users  # ouvre et scanne directement ce dossier
cargo run --release -- --bench C:\Users   # scan sans interface, affiche le débit
cargo test; cargo clippy --all-targets
```

## État

Fonctionnel : scan, arbre, treemap, types, lecteurs (ré-énumérés à la fin de chaque scan), filtre par nom, Ouvrir dans l'Explorateur, Copier le chemin, « Choisir un dossier… » (Ctrl+O, dialogue modal) et clic sur une puce de lecteur (annulent un scan en cours) via une entrée unique `App::open`, dates de la colonne Modifié en heure locale (heure d'été comprise), tailles et compteurs en JetBrains Mono embarquée, taille sur disque par défaut (clusters, fichiers creux, compressés NTFS et WOF, liens physiques comptés une fois), bascule taille logique (Ctrl+T, menu Affichage).
Limites : la toute première ouverture du dialogue après le lancement peut s'ouvrir sur « Ce PC » au lieu du dossier courant ; clé USB, lecteur vide et partage UNC non vérifiés (pas de matériel) ; la validation d'un chemin réseau injoignable est bloquante quelques secondes ; tailles sur disque FAT32/exFAT non vérifiées sur matériel (pas de clé USB) ; un lien physique est attribué au premier nom rencontré pendant le scan (le dossier qui le porte peut changer d'un scan à l'autre, le total non).
Maquettes (rien n'est supprimé sur le disque) : Nettoyer / Examiner et nettoyer, Corbeille, exports, scans enregistrés, et les entrées de menu grisées.
Police : JetBrains Mono (SIL OFL 1.1, `assets/fonts/JetBrainsMono-OFL.txt`).
