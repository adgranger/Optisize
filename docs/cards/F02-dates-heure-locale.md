---
id: F02
title: Dates affichées en heure locale (pas en UTC)
model: sonnet        # modèle qui implémente : sonnet | opus (voir la règle dans ROADMAP.md)
model_reason: Changement borné à src/fmt.rs avec trois appels kernel32 dont les signatures et les structures sont données ici, et des tests déterministes sur un fuseau explicite qui détectent toute erreur de disposition mémoire.
depends_on: []
---

## Contexte

La seule date affichée par l'application est la colonne « Modifié » de l'arbre (`src/ui/tree_view.rs`, `fmt::date(n.mtime, now)`). Chaîne complète aujourd'hui :

1. **Lecture pendant le scan** (`src/scan/mod.rs`, `walk`) : `metadata().modified()` → secondes Unix **UTC** tronquées en `u32` dans `Entry::mtime`, puis `Node::mtime` (`src/scan/tree.rs`). `0` = date inconnue (lecture impossible ou antérieure à 1970). Ce stockage est correct et **ne change pas** : l'UTC est la bonne valeur à garder, seule l'affichage doit convertir.
2. **Horloge** (`src/app.rs`, `App::ui`) : `now` = secondes Unix UTC, recalculé à chaque frame, passé à `TreeView::show`.
3. **Formatage** (`src/fmt.rs`, `date(secs: u32, now: u64)`) : découpe en jours par `secs / 86_400`, donc en jours **UTC**. Affiche `""` si `secs == 0`, « aujourd’hui » si même jour que `now`, « 12 nov. » si même année, sinon « 9 sept. 2023 ».

Défaut : en France (UTC+1 l'hiver, UTC+2 l'été), un fichier modifié entre 00:00 et 01:00 (hiver) ou 02:00 (été), heure locale, s'affiche **à la date de la veille**, et « aujourd’hui » ne bascule qu'à 01:00 ou 02:00 du matin. Aux changements d'année, un fichier du 1er janvier 00:30 apparaît « 31 déc. 2023 ». L'Explorateur Windows et TreeSize affichent l'heure locale.

Le panneau « Sélection » (`src/ui/side.rs`, `selection_panel`) n'affiche **aucune** date aujourd'hui ; rien à y changer (la fenêtre Propriétés, F14, ajoutera les dates détaillées et réutilisera la conversion de cette carte).

## Comportement attendu

### Conversion UTC → locale
- Fuseau = fuseau Windows actif de la session (Paramètres > Heure et langue), **y compris l'heure d'été / d'hiver applicable à la date du fichier**, pas celle d'aujourd'hui : un fichier du 1er juillet 2024 00:30 heure de Paris (22:30 UTC le 30 juin) s'affiche « 1 juil. 2024 » même consulté en janvier.
- Les fuseaux à demi-heure ou quart d'heure (Inde UTC+5:30, Népal UTC+5:45, Terre-Neuve UTC−3:30) sont gérés exactement (pas d'arrondi à l'heure).
- Si l'appel Windows échoue (valeur de retour 0), la valeur UTC est utilisée telle quelle (comportement actuel, aucune panique).
- Hors Windows (`#[cfg(not(windows))]`) : identité, l'heure reste en UTC (le projet garde des branches non-Windows compilables ; pas de nouvelle dépendance pour ce cas).

### Formatage (`fmt::date`)
- Signature publique **inchangée** : `pub fn date(secs: u32, now: u64) -> String`, entrées toujours en secondes Unix UTC. Ainsi `tree_view.rs` et `app.rs` ne changent pas.
- `secs == 0` → `""` (date inconnue), **avant** toute conversion.
- Sinon : convertit `secs` et `now` en local, puis délègue à une fonction pure `fn date_local(local: i64, local_now: i64) -> String` qui contient l'actuelle logique d'affichage.
- Découpe en jours avec `div_euclid(86_400)` sur `i64` (une heure locale peut être négative : `secs = 1` en UTC−5 donne `-17 999`, soit le 31 décembre 1969 → « 31 déc. 1969 »).
- « aujourd’hui » = même jour **local** que `now` (bascule à minuit local). Pas de « hier », pas de « il y a N jours ».
- Même année **locale** que `now` → « 12 nov. » ; sinon « 9 sept. 2023 ».
- **Date future** (horloge du fichier en avance, appareil photo mal réglé) : même règle, sans traitement spécial — même jour local → « aujourd’hui », même année → « 12 déc. », autre année → « 3 mars 2031 ». Pas de « dans N jours ».
- Libellés des mois inchangés (`janv.` … `déc.`, apostrophe typographique `’` dans « aujourd’hui »).
- Le commentaire de doc de `date` devient : « Date en heure locale : « aujourd’hui », « 12 nov. » ou « 9 sept. 2023 ». `secs` et `now` en secondes Unix UTC, `0` = inconnue. »

### Tri
- Le tri de la colonne « Modifié » (`sort_children`, `Col::Date => n.mtime`) reste sur la valeur UTC : l'ordre chronologique est le même. Ne rien changer.

## Choix de l'API

Pas de nouvelle dépendance. `chrono` et `time` ne lisent pas le fuseau Windows sans fonctionnalités supplémentaires (`chrono` + `Local` fonctionne mais ajoute une dépendance et des crates `windows-*` pour trois appels) ; la crate `windows` n'est pas présente. Le projet appelle déjà kernel32 par des blocs `extern "system"` locaux (`src/main.rs` `attach_console` / `quiet_drive_errors`, `src/ui/drives.rs`) : suivre ce modèle.

Fonctions kernel32 (toutes renvoient un `BOOL` = `i32`, 0 en cas d'échec) :

```rust
#[repr(C)] #[derive(Clone, Copy, Default)]
struct FileTime { low: u32, high: u32 }                       // FILETIME

#[repr(C)] #[derive(Clone, Copy, Default)]
struct SystemTime { year: u16, month: u16, day_of_week: u16, day: u16,
                    hour: u16, minute: u16, second: u16, millis: u16 } // SYSTEMTIME

#[repr(C)] #[derive(Clone, Copy)]
struct TimeZoneInfo {                                         // TIME_ZONE_INFORMATION (172 octets)
    bias: i32,
    standard_name: [u16; 32], standard_date: SystemTime, standard_bias: i32,
    daylight_name: [u16; 32], daylight_date: SystemTime, daylight_bias: i32,
}

#[link(name = "kernel32")]
extern "system" {
    fn FileTimeToSystemTime(ft: *const FileTime, st: *mut SystemTime) -> i32;
    fn SystemTimeToTzSpecificLocalTime(tz: *const TimeZoneInfo, utc: *const SystemTime, local: *mut SystemTime) -> i32;
    fn SystemTimeToFileTime(st: *const SystemTime, ft: *mut FileTime) -> i32;
}
```

- Ne pas nommer la structure `SystemTime` dans la portée où `std::time::SystemTime` serait importé (`fmt.rs` n'en importe pas ; sinon la renommer `WinSystemTime`).
- `tz = null()` → fuseau actif de la machine, avec la règle d'heure d'été appliquée à la date convertie. Un pointeur non nul sert uniquement aux tests.
- Ne **pas** utiliser `FileTimeToLocalFileTime` : il applique le décalage **actuel** (hiver ou été d'aujourd'hui) à toutes les dates, ce qui est faux pour la moitié de l'année.
- Conversion Unix ↔ FILETIME (intervalles de 100 ns depuis 1601-01-01 UTC) : `ft = (secs + 11_644_473_600) * 10_000_000` ; retour : `ft / 10_000_000 - 11_644_473_600`. Calcul en `i64` / `u64`, pas de dépassement pour `u32` (jusqu'en 2106).

Fonctions à écrire dans `src/fmt.rs` :

```rust
/// Secondes Unix UTC -> secondes « Unix locales » (même échelle, décalées du fuseau).
fn local(utc: i64) -> i64                                    // = local_with(std::ptr::null(), utc) sous Windows, identité ailleurs
#[cfg(windows)] fn local_with(tz: *const TimeZoneInfo, utc: i64) -> i64  // UTC -> FILETIME -> SYSTEMTIME -> local SYSTEMTIME -> FILETIME -> secondes ; repli sur utc si un appel renvoie 0
fn date_local(local: i64, local_now: i64) -> String          // pure, sans appel système
```

`local` est appelé deux fois par ligne visible et par frame (une trentaine de lignes) : coût négligeable, pas de cache.

## Hors périmètre

- Afficher l'heure (HH:MM) ou la date complète ; ajouter « hier » / dates relatives ; infobulle de date.
- Dates de création ou de dernier accès ; date dans le panneau Sélection (F14).
- Changer le stockage `u32` UTC dans `Entry` / `Node`, ou le tri.
- Réagir à un changement de fuseau en cours d'exécution au-delà de ce que Windows renvoie déjà (l'appel lit le fuseau actif à chaque fois ; rien à ajouter).
- Vue « Fichiers les plus anciens » (F10, qui dépend de cette carte).

## Fichiers concernés

- `src/fmt.rs` (seul fichier de code modifié) :
  - structures `#[repr(C)]` et bloc `extern "system"` ci-dessus, sous `#[cfg(windows)]` ;
  - `fn local(utc: i64) -> i64`, `#[cfg(windows)] fn local_with(...)`, `fn date_local(local: i64, local_now: i64) -> String` ;
  - `pub fn date(secs: u32, now: u64)` : même signature, retourne `""` si `secs == 0`, sinon `date_local(local(secs as i64), local(now as i64))` ;
  - `civil` inchangé (accepte déjà des jours négatifs).
- Aucun changement dans `src/ui/tree_view.rs`, `src/app.rs`, `src/scan/`, `src/ui/side.rs`, `Cargo.toml`.
- `README.md` : section « État », retirer « dates en heure locale » du TODO et indiquer « dates de la colonne Modifié en heure locale (heure d'été comprise) » dans « Fonctionnel ».

## Risques

- **Disposition mémoire des structures** : une erreur (champ manquant, mauvais ordre) donne des dates fausses sans panique. Les tests à fuseau explicite ci-dessous la détectent ; ajouter en plus `assert_eq!(std::mem::size_of::<TimeZoneInfo>(), 172)` et `size_of::<SystemTime>() == 16` dans un test.
- **Règles historiques** : `SystemTimeToTzSpecificLocalTime` applique la règle d'heure d'été courante du fuseau à toutes les années. Pour un fuseau dont les règles ont changé (Russie 2011–2014, États-Unis avant 2007), une date ancienne peut être décalée d'une heure, donc d'un jour seulement si l'heure du fichier est à moins d'une heure de minuit. Limite acceptée, à noter dans la doc de `local` (`shortcut:` — passer à `SystemTimeToTzSpecificLocalTimeEx` + `GetDynamicTimeZoneInformation` si un utilisateur le signale).
- **Volumes FAT32 / exFAT** (clés USB) : ils stockent l'heure locale, Windows la convertit en UTC avec le décalage courant avant de nous la rendre. Une heure d'écart possible selon la saison d'écriture : comportement de Windows, identique dans l'Explorateur, rien à corriger.
- **Heure ambiguë au passage à l'heure d'hiver** (02:00–03:00 vécu deux fois) : sans effet visible, seule la date est affichée.
- **Tests dépendants de la machine** : ne jamais comparer une sortie de `local(...)` (fuseau actif) à une valeur fixe ; seuls les tests avec `local_with(&tzi, ...)` ou `date_local` ont des attentes exactes.

## Critères de fin

- `cargo build --release`, `cargo clippy --all-targets` et `cargo test` passent sans avertissement.
- Tests unitaires dans `src/fmt.rs` `mod tests`. Fuseaux de test construits dans le test :
  - **Paris** : `bias = -60`, `standard_bias = 0`, `daylight_bias = -60`, `standard_date = SystemTime { month: 10, day_of_week: 0, day: 5, hour: 3, ..Default::default() }` (dernier dimanche d'octobre 03:00), `daylight_date = SystemTime { month: 3, day_of_week: 0, day: 5, hour: 2, .. }` (dernier dimanche de mars 02:00), noms à zéro.
  - **Inde** : `bias = -330`, `standard_date` et `daylight_date` à zéro (`month == 0` = pas d'heure d'été), biais standard et été à 0.
  - Repères UTC utiles : 2024-01-01 00:00 = `1_704_067_200` ; 2024-03-31 01:00 = `1_711_846_800` ; 2024-07-01 00:00 = `1_719_792_000`.

  Tests sans appel système (toutes plateformes) :
  - `date_local_today` — `date_local(t, t + 3600)` avec `t` = 2024-07-01 10:00 → « aujourd’hui ».
  - `date_local_same_year` — 2024-11-12 vs maintenant 2024-12-01 → « 12 nov. ».
  - `date_local_other_year` — 2023-09-09 vs 2024-12-01 → « 9 sept. 2023 ».
  - `date_local_future` — 2031-03-03 vs 2024-12-01 → « 3 mars 2031 » ; 2024-12-20 vs 2024-12-01 → « 20 déc. ».
  - `date_local_negative` — `date_local(-17_999, 0)` → « 31 déc. 1969 ».
  - `date_zero_is_empty` — `date(0, n)` → `""` pour n'importe quel `n`.
  - `formats` (existant) conservé.

  Tests Windows (`#[cfg(windows)]`, attentes exactes grâce au fuseau explicite) :
  - `tz_struct_sizes` — 172 et 16 octets.
  - `paris_winter_offset` — `local_with(&paris, 2024-01-15 12:00 UTC)` = même instant + 3 600.
  - `paris_summer_offset` — `local_with(&paris, 2024-07-15 12:00 UTC)` = + 7 200.
  - `paris_dst_switch` — `1_711_846_799` (00:59:59 UTC le 31 mars 2024) → + 3 600 ; `1_711_846_800` (01:00:00 UTC) → + 7 200.
  - `paris_midnight_summer` — 2024-06-30 22:30 UTC → `date_local(local_with(&paris, t), local_with(&paris, 2024-07-01 10:00 UTC))` = « aujourd’hui » (l'ancien code donnait « 30 juin »).
  - `paris_new_year` — 2023-12-31 23:30 UTC, maintenant 2024-03-01 → « 1 janv. » (l'ancien code donnait « 31 déc. 2023 »).
  - `india_half_hour` — 2024-01-01 20:00 UTC → `local_with(&india, t) - t == 19_800` et la date affichée est le 2 janvier.
  - `local_active_zone_is_sane` — pour `t = now` : `(local(t) - t)` est un multiple de 900 et compris dans `[-14 h, +14 h]` (sans valeur exacte, indépendant de la machine).
- Vérification manuelle (`cargo run --release`), dans un dossier de test neuf créé sous `$env:TEMP` (ne toucher à aucun autre fichier) :
  ```powershell
  $d = New-Item -ItemType Directory "$env:TEMP\optisize-f02-$(Get-Random)"
  $a = New-Item "$d\minuit.txt"; $a.LastWriteTime = (Get-Date).Date.AddMinutes(30)
  $b = New-Item "$d\ete.txt";    $b.LastWriteTime = Get-Date '2024-07-01 00:30'
  $c = New-Item "$d\hiver.txt";  $c.LastWriteTime = Get-Date '2024-01-15 00:30'
  ```
  1. Scanner `$d` : `minuit.txt` → « aujourd’hui », `ete.txt` → « 1 juil. 2024 », `hiver.txt` → « 15 janv. 2024 » (sur une machine à l'est de Greenwich ; avant le correctif, les deux derniers affichaient le 30 juin et le 14 janvier).
  2. Comparer quelques fichiers avec la colonne « Modifié le » de l'Explorateur : mêmes dates.
  3. Le tri par « Modifié » reste chronologique.
  4. Ne pas changer le fuseau horaire de Windows pour tester (réglage système) ; les tests à fuseau explicite couvrent les autres fuseaux.
  5. Supprimer ensuite le seul dossier `$d` créé ci-dessus.
- Lignes à mettre à jour : statut F02 → `done` dans `ROADMAP.md` ; section « État » du `README.md` comme indiqué dans « Fichiers concernés ».
