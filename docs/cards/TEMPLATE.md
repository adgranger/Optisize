---
id: F00
title: Titre court de la fonctionnalité
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: une phrase qui justifie ce choix
depends_on: []
---

## Contexte
Pourquoi cette fonctionnalité existe, ce que l'utilisateur voit aujourd'hui, la référence dans la maquette ou dans TreeSize.

## Comportement attendu
Liste précise, testable, des comportements (états, cas limites, messages en français).

## Hors périmètre
Ce que la tâche ne doit pas faire.

## Fichiers concernés
Modules à modifier ou à créer (`src/...`), et ce qu'ils doivent exposer.

## Risques
Données, performance, compatibilité Windows, points à valider avant de coder.

## Critères de fin
- `cargo build --release`, `cargo clippy --all-targets` et `cargo test` passent sans avertissement.
- Tests nommés à écrire (au moins un par comportement non trivial).
- Vérification manuelle à faire dans l'application.
- Lignes à mettre à jour : statut dans `ROADMAP.md`, section « État » du `README.md`.
