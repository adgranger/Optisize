# Optisize

Analyseur d'espace disque pour Windows dans l'esprit de TreeSize Free, écrit en Rust (egui/eframe). Interface en français.

- Scan parallèle en arrière-plan, l'arborescence se remplit en direct (le thread UI ne bloque jamais).
- Arbre compact (arène `Vec<Node>` + pool de noms), tableau virtualisé triable, treemap imbriqué, répartition par type, lecteurs.
- Pause / Reprendre / Arrêter / Rafraîchir. Les dossiers inaccessibles sont ignorés.

## Lancer

```powershell
cargo run --release              # ouvre l'application
cargo run --release -- C:\Users  # ouvre et scanne directement ce dossier
cargo run --release -- --bench C:\Users [--json] [--profile]   # scan sans interface : débit, ligne JSON, phases
pwsh bench\bench.ps1             # benchmark reproductible (A/B, TreeSize/WizTree si présents) : voir docs/bench.md
cargo test; cargo clippy --all-targets -- -D warnings   # mêmes commandes que la CI (.github/workflows/ci.yml)
```

## État

Fonctionnel : scan, arbre, treemap, types, lecteurs (ré-énumérés à la fin de chaque scan), filtre par nom, Ouvrir dans l'Explorateur, Copier le chemin, « Choisir un dossier… » (Ctrl+O, dialogue modal) et clic sur une puce de lecteur (annulent un scan en cours) via une entrée unique `App::open`, dates de la colonne Modifié en heure locale (heure d'été comprise), tailles et compteurs en JetBrains Mono embarquée, taille sur disque par défaut (clusters, fichiers creux, compressés NTFS et WOF, liens physiques comptés une fois), bascule taille logique (Ctrl+T, menu Affichage), `--bench` avec dossiers ignorés, racine refusée (code 3), sortie `--json` et phases `--profile`, script `bench\bench.ps1` (protocole, arbre synthétique, A/B, TreeSize Free mesuré au temps CPU), scan à 8 workers en parcours en profondeur (chiffres dans `docs/bench.md`), toast « Accès refusé : … » quand la racine est illisible, CI GitHub Actions sous Windows Server 2025 (clippy sans avertissement, tests, build release, auto-test du bench) à chaque push sur main et chaque pull request, vues Détails / Carte / Types (F6–F8, onglets, menu Affichage ; sélection, tri et défilement conservés), zoom du treemap (double-clic, fil d'Ariane cliquable, remonter par ⬆ / Retour arrière / bouton souris 4).
Limites : la toute première ouverture du dialogue après le lancement peut s'ouvrir sur « Ce PC » au lieu du dossier courant ; clé USB, lecteur vide et partage UNC non vérifiés (pas de matériel) ; la validation d'un chemin réseau injoignable est bloquante quelques secondes ; tailles sur disque FAT32/exFAT non vérifiées sur matériel (pas de clé USB) ; un lien physique est attribué au premier nom rencontré pendant le scan (le dossier qui le porte peut changer d'un scan à l'autre, le total non) ; `C:\Program Files\WindowsApps` : racine refusée sans élévation aux applications non empaquetées (ACL Windows) ; ses sous-dossiers restent analysables un par un ; WizTree (absent), cache froid et profil WPR non mesurés (étapes utilisateur dans `docs/bench.md`) ; premier run de la CI à faire par l'utilisateur (rien n'est poussé par les sessions) ; vues F07 : implémentation vérifiée par tests unitaires et lancement sans panique, mais pas encore à l'écran (fluidité du grand treemap pendant un scan, touches F6–F8 avec un champ texte focalisé, conservation du défilement : à vérifier) ; zoom F08 : logique vérifiée par 13 tests unitaires et lancement sans panique, mais pas encore à l'écran (double-clic, glyphe ⬆, bouton souris 4, fil d'Ariane tronqué dans la colonne de 360 px, Retour arrière avec un champ focalisé : à vérifier).
Maquettes (rien n'est supprimé sur le disque) : Nettoyer / Examiner et nettoyer, Corbeille, exports, scans enregistrés, et les entrées de menu grisées.
Police : JetBrains Mono (SIL OFL 1.1, `assets/fonts/JetBrainsMono-OFL.txt`).
