---
name: next-task
description: Passe à la tâche suivante de la roadmap Optisize en la confiant à une sous-session. Écrit la carte en Opus quand elle manque, sinon implémente la carte avec le modèle (Sonnet ou Opus) qu'elle prévoit. À lancer avec /next-task, optionnellement suivi d'un identifiant (par exemple /next-task F20).
---

# next-task

Fait avancer `ROADMAP.md` d'un pas, dans une sous-session. Tu ne codes pas toi-même : tu choisis, tu briefes, tu vérifies.

## 1. Choisir la tâche

1. Lis `ROADMAP.md` (cycle de vie et tableaux).
2. Si l'utilisateur a donné un identifiant (`F20`), prends celui-là. Sinon prends la première ligne, dans l'ordre du fichier, dont le statut n'est pas `done` et dont toutes les dépendances sont `done`.
3. Si une ligne est `doing`, ne démarre rien d'autre : demande si on reprend cette tâche.
4. Si aucune ligne n'est éligible, dis-le et nomme ce qui bloque.
5. Annonce en une ligne : identifiant, titre, type de tâche (écriture de carte ou implémentation), modèle.

## 2. Type de tâche et modèle

| Statut de la ligne | Tâche | Modèle de la sous-session |
|---|---|---|
| `todo` | **écrire la carte** `docs/cards/<ID>-<slug>.md` depuis `docs/cards/TEMPLATE.md` | **Opus**, sans exception (`claude-opus-5-5`) |
| `ready` ou `doing` | **implémenter** la carte | le champ `model` de l'en-tête de la carte ; à défaut, la colonne « Modèle dev » de `ROADMAP.md` (`sonnet` = `claude-sonnet-5-5`, `opus` = `claude-opus-5-5`) |

`start_session` ignore un modèle plus puissant que celui de la session courante. Si le modèle requis est Opus et que cette session ne tourne pas sur Opus, **n'abaisse pas** la tâche en Sonnet : dis-le à l'utilisateur et demande-lui de passer cette session sur Opus (sélecteur de modèle) ou de relancer `/next-task` depuis une session Opus.

## 3. Avant de démarrer

- `rtk git status` : l'arbre doit être propre. Sinon, signale-le et arrête-toi.
- Une seule sous-session qui écrit du code à la fois. Vérifie `list_sessions` avec `linked: true`.
- Branche : `feat/<ID>-<slug>` pour une implémentation, `docs/<ID>-card` pour une carte. La sous-session la crée depuis la branche principale à jour.

## 4. Lancer la sous-session

Utilise `start_session` (`initiation: "user_asked"`, `context: "fresh"`, `cwd` = la racine du dépôt, `use_worktree: false`, `model` selon le tableau, `wake_when_done: true`). Le brief commence par cette ligne exacte :

> Every shell command, git included, MUST start with `rtk` (use `rtk bypass <cmd>` only if the condensed output is unusable).

### Brief d'écriture de carte (Opus)

- Écris `docs/cards/<ID>-<slug>.md` d'après `docs/cards/TEMPLATE.md`, pour la ligne `<ID>` de `ROADMAP.md`. Lis d'abord `README.md`, `ROADMAP.md` et le code des modules concernés.
- Remplis le champ `model` (sonnet ou opus) avec une justification d'une phrase, d'après la règle de `ROADMAP.md`.
- Une carte doit être assez précise pour qu'un autre modèle l'implémente sans poser de question : comportements testables, fichiers, cas limites, risques, tests nommés.
- Ne touche pas au code source. Mets à jour la ligne `<ID>` de `ROADMAP.md` : colonne « Carte » = le chemin du fichier, « Modèle dev » = la valeur de la carte, statut = `ready`.
- Commit unique, signé, sans ligne `Co-Authored-By`, sur la branche `docs/<ID>-card`.

### Brief d'implémentation

- Implémente la carte `docs/cards/<ID>-<slug>.md`, rien d'autre. Ce qui est « Hors périmètre » reste hors périmètre.
- Passe la ligne `<ID>` de `ROADMAP.md` à `doing` au début, à `done` à la fin seulement si tous les critères de fin sont remplis.
- Termine par `cargo build --release`, `cargo clippy --all-targets` et `cargo test` propres, plus la vérification manuelle de la carte (lance l'application). Écris les tests que la carte nomme.
- Mets à jour la section « État » du `README.md`.
- Commits signés sur la branche `feat/<ID>-<slug>`, **sans ligne `Co-Authored-By`** ni mention de Claude dans le message. Si la signature GPG échoue, arrête-toi et signale-le : ne la désactive pas.
- **Ne pousse pas**, ne crée ni dépôt distant ni pull request, ne supprime aucun fichier de l'utilisateur sur le disque en dehors de ce que la carte décrit.
- Rapport final : ce qui marche, ce qui est vérifié et comment, ce qui reste ou a été reporté.

## 5. Après la sous-session

1. Lis son rapport, puis vérifie toi-même : `rtk git log --oneline -5`, le diff et, pour une implémentation, `rtk cargo test` et `rtk cargo clippy --all-targets`. Un rapport n'est pas une preuve.
2. Si la carte ou le code est incomplet, renvoie la sous-session corriger (`SendMessage`) au lieu d'en ouvrir une autre.
3. Si c'est bon, vérifie que le statut dans `ROADMAP.md` est cohérent (`ready` après une carte, `done` après une implémentation vérifiée).
4. Dis à l'utilisateur en trois lignes : ce qui a été fait, la branche, et la tâche suivante que `/next-task` choisirait. Ne la lance pas sans qu'il le demande. Ne fusionne ni ne pousse la branche sans accord explicite.
