# Translation glossary

The fixed terms for every term the desktop UI shares with RimWorld,
plus the terms Rimmerge has that the game doesn't. A translator uses
the term in this table every time the concept appears, in labels and in
sentences alike. [`docs/translating.md`](../../../../docs/translating.md)
explains how the table fits into the rest of the process.

## How to read this table

- **Status `game`**: the term is the one RimWorld's own official
  translation uses. It was read from the game's language archives, one
  key at a time, and only as a term: never a whole sentence. The
  Ludeon and volunteer translators own those files; Rimmerge takes
  single words and short labels from them, as a glossary.
- **Status `proposed`**: the game has no term for this concept. The
  entry is Rimmerge's proposal, and its note says why this word was
  picked. A native reviewer may replace it by opening an issue.
- **Source** names where a `game` term came from:
  `<pack> · <archive member> · <key>`. The pack is the folder under
  `RimWorld/Data/` (`Core`, `Biotech`, …). The member lives in both
  `Languages/ChineseSimplified (简体中文).tar` and
  `Languages/PortugueseBrazilian (Português Brasileiro).tar` of that
  pack, under the same key. `Languages/English/` gives the English
  side of a `Keyed` pair; for a `DefInjected` key, the English side is
  the def's own `label` under `Data/<pack>/Defs/`.
- A term in `code` style is literal text, identical in every language.
- **Filled from the first full round.** Each locale's column holds the
  term its translator settled on, taken from the translators' own
  terminology notes and checked against the shipped `<locale>.json`
  (the catalogue wins where a note and the file disagree). The Status
  column names the locales whose term is `proposed` rather than `game`;
  a bare `game` means no translator reported a deviation. Where translators chose different
  terms for one English concept, every locale lists its own term: the table
  records what each catalogue says and does not pick a winner. "Other
  locales:" at the end of a row's last cell holds a deviation or a
  caveat that does not fit a cell.
- **Empty cells.** A cell that stays empty means the concept never came up
  in that language's catalogue. A new term goes into that locale's column
  only: a `game` term from the official language archive where the Status
  says so, otherwise a proposal with its reason in the translator's report.
  Never edit another locale's column.

## House style the game confirms

- **zh-CN keeps "Mod" in Latin script.** The key
  `Core · Keyed/Menus_Main.xml · Mod` reads `Mod` in the Chinese
  archive. The newer game strings put a half-width space between Han
  characters and the Latin word (`Mod 设置`, `该 Mod`); do the same.
  Chinese has no plural, so "3 mods" is `3 个 Mod`.
- **zh-CN addresses the user as 你, not 您.** Across the base game and
  the expansions, the Chinese Keyed files use 你 in 582 strings and 您
  in 6. Most UI labels need neither pronoun.
- **zh-CN uses full-width punctuation** (`：`, `，`, `。`, `（）`)
  around Han text.
- **zh-CN puts no space around a placeholder whose value is always Han
  text** (`{placement}`, `{placementPhrase}`, `{source}`, `{order}`,
  `{layerPhrase}`, `{rulePhrase}`): `被固定在{placement}`, not
  `被固定在 {placement}`. The half-width space above is for Latin words,
  mod names and numbers. A Han edge-kind label (`{edgeKind}`,
  `{relationKind}`) spliced into a sentence is parenthesized or quoted:
  `舍弃排序约束 {after} → {before}（{edgeKind}）`, `“{relationKind}”关系`.
- **pt-BR addresses the user as você.** The same Portuguese Keyed files
  use você in 410 strings and never tu.
- **pt-BR uses "mod" as a masculine loanword**: `o mod`, `os mods`,
  `Este mod é incompatível com…` (`Core · Keyed/Menus_Main.xml ·
  ModIncompatibleWithTip`). It is lowercase inside a sentence.
- **pt-BR labels use sentence case**, like the English catalogue. The
  game capitalizes many labels in Title Case (`Abrir Pasta`); that is
  not a pattern to copy.
- **Address in the other locales** (from each translator's notes): ru
  *вы*, uk *ви*, de *du*, fr *vous*, es-ES *tú*, zh-TW *你* (all lowercase
  mid-sentence, matching their Core Keyed files); pl *ty* with
  imperative buttons (`Zapisz`, `Włącz`); tr *sen* with bare imperative
  buttons (`Kaydet`, `Sil`), as in the game's Core Keyed files.
- **Mod gender and spelling.** de *die Mod* (feminine), pt-BR *o mod*
  (masculine), ja `MOD` in capitals with no space against Japanese text,
  zh-TW *模組* (never Latin `Mod`), ko *모드*. A placeholder that carries
  a mod name never takes a case ending or suffix: the sentence adds a
  noun instead (ru, uk *мод {mod}*, pl *mod {mod}*, tr *{mod} modu*,
  ko *{mod} 모드*).

## Mods and the mod list

| English | zh-CN | pt-BR | ru | uk | pl | de | fr | es-ES | tr | ja | ko | zh-TW | Status | Source or reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| mod | Mod | mod | мод | мод | mod | Mod (die Mod) | mod | mod | mod | MOD | 모드 | 模組 | game | Core · Keyed/Menus_Main.xml · Mod. Other locales: ru/uk/pl/fr/es/tr: lowercase inside a sentence. de: feminine `die Mod` (Core · Keyed/Menus_Main.xml · ModMustLoadAfter). ja: `MOD` in capitals (Core Japanese Keyed has MOD 64 times against Mod 20). uk: the game's Menus_Main writes `Модифікація`, but its own ModIncompatibleWithTip/MissingModsList say `мод`; `мод` is used. tw: 模組, never Latin `Mod`. |
| Mods (page or menu title) | Mod | Mods | Моды | Моди | Mody | Mods | Mods | Mods | Modlar | MOD | 모드 | 模組 | game | Core · Keyed/Menus_Main.xml · Mods. The game's menu entry is `Mod 配置` ("mod configuration"); a page listing mods reads better as `Mod` or `Mod 列表`. |
| mod list | Mod 列表 | lista de mods | список модов | список модів | lista modów | Mod-Liste | liste de mods | lista de mods | mod listesi | MODリスト | 모드 목록 | 模組清單 | game | Core · Keyed/Dialogs_Various.xml · ModsChanged |
| active (a mod's state) | 已启用 | ativo | активный | активний | aktywny | aktiv | actif | activo | etkin | 有効 | 활성 | 已啟用 | game | Core · Keyed/Dialogs_Various.xml · LoadingAllActiveMods (`启用的 Mod`); ContentActive (`Ativo`). The game also writes 激活 in places; use 启用 so "active" and "activate" share one word. |
| inactive | 未启用 | inativo | неактивный | неактивний | nieaktywny | inaktiv | inactif | inactivo | etkin olmayan | 無効 | 비활성 | 未啟用 | game | Core · Keyed/Misc_Gameplay.xml · InactiveFacility (`inativo`); zh follows 启用 above (the game's 未激活 in ContentInstalledButNotActive uses the other verb) |
| activate (a mod) | 启用 | ativar | включить | увімкнути | włącz | aktivieren | activer | activar | etkinleştir | 有効化 | 활성화 | 啟用 | game | Core · Keyed/Menus_Main.xml · Enable |
| deactivate (a mod) | 禁用 | desativar | отключить | вимкнути | wyłącz | deaktivieren | désactiver | desactivar | devre dışı bırak | 無効化 | 비활성화 | 停用 | game | Core · Keyed/Menus_Main.xml · Disable |
| enabled / disabled (a setting) | 启用 / 禁用 | ativado / desativado | включено / отключено | увімкнено / вимкнено | włączone / wyłączone | aktiviert / deaktiviert | activé / désactivé | activado / desactivado | etkin / devre dışı | 有効 / 無効 | 켜짐 / 꺼짐 | 已啟用 / 已停用 | game | Core · Keyed/Menus_Main.xml · Enabled, Disabled |
| installed | 已安装 | instalado | установлен | встановлений | zainstalowany | installiert | installé | instalado | yüklü | インストール済み | 설치됨 | 已安裝 | game | Core · Keyed/Dialogs_Various.xml · ContentInstalledButNotActive |
| missing (a mod) | 缺失 | ausente | отсутствующий | відсутній | brakujący | fehlend | manquant | faltante | eksik | 見つからない | 누락된 | 缺少 | game | Core · Keyed/Menus_Main.xml · MissingModsList. Other locales: ja: `見つからない`; the game's `削除されたMOD` means "deleted" and was rejected. |
| incompatible | 不兼容 | incompatível | несовместимый | несумісний | niekompatybilny | inkompatibel | incompatible | incompatible | uyumsuz | 非互換 | 호환되지 않음 | 不相容 | game | Core · Keyed/Menus_Main.xml · Incompatible |
| dependency | 依赖 | dependência | зависимость | залежність | zależność | Abhängigkeit | dépendance | dependencia | bağımlılık | 依存関係 | 종속성 | 依賴 | game | Core · Keyed/Menus_Main.xml · ModCyclicDependency |
| cyclic dependency, cycle | 循环依赖 | dependência cíclica | циклическая зависимость, цикл | цикл | cykl | zyklische Abhängigkeit, Zyklus | dépendance cyclique, cycle | ciclo | döngüsel bağımlılık, döngü | 循環 | 순환 종속성, 순환 | 循環依賴, 循環 | game | Core · Keyed/Menus_Main.xml · ModCyclicDependency |
| requires | 需要 | requer | требует | потребує | wymaga | benötigt | nécessite | necesita | gerektiriyor | 必要 | 필요 | 需要 | game | Core · Keyed/Misc_Gameplay.xml · Requires |
| load after {mod} / load before {mod} | 在 {mod} 之后加载 / 在 {mod} 之前加载 | carregar depois de {mod} / carregar antes de {mod} | загружать после / загружать до | завантажувати після / завантажувати перед | ładowanie po / ładowanie przed | laden nach / laden vor | charger après / charger avant | cargar después / cargar antes | sonra yükle / önce yükle | 後にロード / 前にロード | 뒤에 로드 / 앞에 로드 | 在後載入 / 在前載入 | game | Core · Keyed/Menus_Main.xml · ModMustLoadAfter, ModMustLoadBefore, ModReorderConflict_MustLoadBefore. Other locales: ko: 로드 is the translator's choice (matches the noun 로드 순서); the game's ModMustLoadAfter uses `불러와야`. ja: ロード for mods, 読み込む for the app loading a file or project. ja: `後に`/`前に` (`{after}を{before}の後にロード`); `前でロード` is spatial and ungrammatical for an order. |
| auto-sort | 自动排序 | ordenar automaticamente |  |  |  | automatisch sortieren |  |  | modları otomatik sırala | 自動並べ替え | 자동 정렬 | 自動排序 | game (zh), proposed (pt) | Core · Keyed/Menus_Main.xml · ResolveModOrder. The pt-BR game label is `Organizar Mods`, which drops "auto"; `ordenar` matches the game's `Ordenar por` (Dialogs_Various · SortBy). |
| sort (verb) | 排序 | ordenar |  | сортувати |  | sortieren | trier | ordenar | sırala | 並べ替え | 정렬 | 排序 | game | Core · Keyed/Dialogs_Various.xml · SortBy |
| package id, ID | ID | ID | ID пакета, ID | ID пакета, ID | ID pakietu | Package-ID, ID | ID du paquet, ID | ID de paquete | Paket ID'si | パッケージID | 패키지 ID | Package ID | game | Core · Keyed/Menus_Main.xml · ModPackageId. The literal field name `packageId` stays in `code`. Other locales: tw: the game's ModPackageId is a bare `ID`; the longer `Package ID` avoids a clash with Workshop ID. |
| author | 作者 | autor | автор | автор | autor | Autor | auteur | autor | yapan (label), mod yapımcısı (prose) | 作者 | 저작자 | 作者 | game | Core · Keyed/Menus_Main.xml · Author (`Autor(es)`) |
| mod version | Mod 版本 | versão do mod | версия мода | версія мода | wersja moda | Mod-Version | version de mod | versión del mod | mod sürümü | MODバージョン | 모드 버전 | 模組版本 | game | Core · Keyed/Menus_Main.xml · ModVersion |
| game version | 游戏版本 | versão do jogo | версия игры | версія гри | wersja gry | Spielversion | version du jeu | versión del juego | oyun sürümü | ゲームバージョン | 게임 버전 | 遊戲版本 | game | Core · Keyed/Menus_Main.xml · ModTargetVersion |
| Mod settings | Mod 设置 | configurações do mod |  |  |  | Mod-Einstellungen | paramètres des mods | opciones de mods |  |  | 모드 설정 | 模組設定 | game | Core · Keyed/Menu_Options.xml · ModSettings. pt uses the plural `configurações`, as in Menus_Main · AdvancedSettings. |

## Where mods come from

| English | zh-CN | pt-BR | ru | uk | pl | de | fr | es-ES | tr | ja | ko | zh-TW | Status | Source or reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Steam Workshop | Steam 创意工坊 | Oficina | Мастерская Steam | Майстерня Steam | Warsztat Steam | Steam Workshop | Steam Workshop | Steam Workshop | Steam Atölyesi | Steamワークショップ | 스팀 창작마당 | Steam 創意工坊 | game | Core · Keyed/Enums.xml · ContentSource_SteamWorkshop |
| Workshop | 创意工坊 | Oficina | Мастерская | Майстерня | Warsztat | Workshop | Workshop | Workshop | Atölye | ワークショップ | 창작마당 | 創意工坊 | game | Core · Keyed/Menus_Main.xml · FromWorkshop |
| Workshop ID | 工坊ID | ID da Oficina |  | ID Майстерні |  | Workshop-ID | ID Workshop | Workshop ID | Atölye ID | ワークショップID | 창작마당 ID | 創意工坊 ID | game (proposed in tw) | Core · Keyed/Menus_Main.xml · WorkshopId. Other locales: tw: the game's ChineseTraditional `WorkshopId` reads `工作坊 ID`, which clashes with the game's own 創意工坊 (Enums · ContentSource_SteamWorkshop) and with Steam's Taiwan client; 創意工坊 ID is proposed instead. |
| local folder (a mod source) | 本地文件 | pasta local | локальный | локальний | lokalny | lokaler Ordner | dossier local | carpeta local | yerel klasör | ローカルフォルダー | 로컬 (game: 개인 모드 폴더) | 本機資料夾 | game | Core · Keyed/Enums.xml · ContentSource_ModsFolder |
| official content | 官方内容 | conteúdo oficial |  | офіційний контент |  | offizieller Inhalt | contenu officiel | contenido oficial | resmî içerik | 公式コンテンツ |  | 官方內容 | game | Core · Keyed/Enums.xml · ContentSource_OfficialModsFolder |
| expansion, DLC | 扩展 (官方扩展 for "official expansion") | expansão (expansão oficial) | дополнение | розширення |  | Erweiterung | extension | expansión | genişletme | DLC (game: 公式DLC) | 확장팩 | 擴充 / 官方擴充 | game | Core · Keyed/Dialog_StatsReports.xml · Stat_Source_OfficialExpansionReport. The game never says "DLC"; use the expansion term, and keep `DLC` only where the English names the sorter tier (see "tier" below). Other locales: ru/pl/de/fr/tr/ja keep `DLC` where the English names the sorter tier (fr deliberately, for the tier badge). fr: `extension` (also source DLC). tw: the game says 官方擴展包, Taiwan software says 擴充. ko: 확장팩 everywhere, including the sorter tier badge (`order.tier.dlc`), so the tier and the Mods source filter name the same mods alike. |
| Core (the base game, in prose) | 核心 | Núcleo | Core | Core | Core | Core | Core | Core | Ana İçerik | Core | 코어 | Core | game | Core · DefInjected/ExpansionDef/ExpansionDefs.xml · Core.label |
| Royalty / Ideology / Biotech / Anomaly / Odyssey (in prose) | 皇权 / 文化 / 生物技术 / 异象 / 奥德赛 | Royalty / Ideology / Biotech / Anomaly / Odyssey |  | Royalty / Ideology / Biotech / Anomaly / Odyssey (English) |  | Royalty / Ideology / Biotech / Anomaly / Odyssey (English) |  | Royalty / Ideology / Biotech / Anomaly / Odyssey (English) |  | Royalty / Ideology / Biotech / Anomaly / Odyssey (English) | 로얄티 / 이데올로기 / 바이오테크 / 아노말리 / 오디세이 | 皇權 / 理念 / 生機 / 異邪 / 漫遊 | game | Core · DefInjected/ExpansionDef/ExpansionDefs.xml · `<name>.label`. pt-BR keeps the English names. |
| folder | 文件夹 | pasta | папка | тека | folder | Ordner | dossier | carpeta | klasör | フォルダー | 폴더 | 資料夾 | game | Core · Keyed/Menus_Main.xml · OpenModsDataFolder. Other locales: pl: the Polish archive reads `Otwórz folder z modami` (OpenModsDataFolder) and `Otwórz folder` (ModFolder); English "directory" (setup and export path fields) = `katalog`. |
| open folder | 打开目录 | abrir pasta |  | відкрити теку |  | Ordner öffnen |  | abrir carpeta |  |  | 폴더 열기 |  | game | Core · Keyed/Menus_Main.xml · ModFolder |

A mod's own display name, including the name the base game and the
expansions give themselves in `About.xml`, is mod-authored text and is
never translated. The prose names above are for sentences that talk
*about* an expansion.

## Game content that findings mention

| English | zh-CN | pt-BR | ru | uk | pl | de | fr | es-ES | tr | ja | ko | zh-TW | Status | Source or reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| texture | 纹理 | textura | текстура | текстура | tekstura | Textur | texture | textura | doku | テクスチャ | 텍스처 | 紋理 | proposed (zh), game (pt) | Core · Keyed/Menu_Options.xml · TextureCompression (`Compressão de Textura`). The zh-CN game label is `压缩画质` ("image-quality compression"), which paraphrases and has no standalone word; 纹理 is the standard graphics term. |
| terrain | 地形 | terreno | поверхность |  | teren |  | Terrain | terreno |  |  | 지형 | 地形 | game (ru, pt), proposed (zh, tw, pl, fr, es, ko) | Core · Keyed/Designators.xml · TerrainCannotSupport (ru: `…на этой поверхности`). ru: never `покрытие`, which is "coverage". Other locales: pt: the PortugueseBrazilian archive under the same key reads `terreno`. |
| research | 研究 | pesquisa |  |  |  | Forschung | recherche | investigación | araştırma |  | 연구 | 研究 | game | Core · DefInjected/MainButtonDef/MainButtons.xml · Research.label |
| faction | 派系 | facção |  |  |  |  | faction | facción | fraksiyon |  | 세력 | 派系 | game | Core · Keyed/Misc_Gameplay.xml · Faction |
| scenario | 剧本 | cenário | сценарий |  |  |  | scénario | escenario | senaryo |  | 시나리오 | 劇本 | game | Core · Keyed/Dialog_StatsReports.xml · StatsReport_ScenarioFactor |
| development mode | 开发人员模式 | modo desenvolvedor | режим разработчика | режим розробника | tryb dewelopera | Entwicklermodus | mode développement | modo desarrollador | geliştirici modu |  | 개발자 도구 | 開發者模式 | game | Core · Keyed/Menu_Options.xml · DevelopmentMode |
| log | 日志 | registro | журнал | журнал | log | Log | journal | registro | günlük | ログ | 로그 | 日誌 | game (proposed in de, ko, pl) | Core · Keyed/ITabs.xml · TabLog. The file name `Player.log` stays in `code`. Other locales: de and ko knowingly write the loanword (`Log`, `로그`) because the game's `Logbuch`/`일지` is the pawn log tab; pl also writes `log`. Compounds: de `Spiel-Log`, pl `log gry`, ko `게임 로그`. |
| warning | 警告 | aviso | предупреждение |  | ostrzeżenie | Warnung | avertissement | advertencia | uyarı | 警告 | 경고 | 警告 | game | Core · Keyed/Misc_Gameplay.xml · Warning |

## Actions and common words

| English | zh-CN | pt-BR | ru | uk | pl | de | fr | es-ES | tr | ja | ko | zh-TW | Status | Source or reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| apply | 应用 | aplicar | Применить | Застосувати | Zastosuj | Anwenden | Appliquer | Aplicar | Uygula (noun, one apply run: Uygula işlemi) | 適用 | 적용 | 套用 | game (proposed in pl) | Biotech · Keyed/Dialogs_Various.xml · SaveAndApply. Other locales: tr: never the noun `uygulama`, which is the catalogue's word for "the app". |
| save | 保存 | salvar | Сохранить | Зберегти | Zapisz | Speichern | Enregistrer | Guardar | Kaydet | 保存 | 저장 | 儲存 | game (proposed in fr, ja, tw) | Core · Keyed/Menus_Main.xml · Save. Other locales: fr: the game says `Sauver`; `Enregistrer` is the project's choice. ja: the game's `セーブ` means a savegame. tw: Core writes 保存, Taiwan software 儲存. |
| cancel | 取消 | cancelar | Отменить | Скасувати | Anuluj | Abbrechen | Annuler | Cancelar | İptal | キャンセル | 취소 | 取消 | game | Core · Keyed/Dialogs_Various.xml · Cancel |
| close | 关闭 | fechar | Закрыть | Закрити | Zamknij | Schließen | Fermer | Cerrar | Kapat | 閉じる | 닫기 | 關閉 | game | Core · Keyed/Dialogs_Various.xml · Close |
| OK | 确定 | OK |  | ОК | OK | OK | OK | OK | Tamam |  | 확인 | 確定 | game | Core · Keyed/Dialogs_Various.xml · OK |
| confirm | 确定 | confirmar | Подтвердить | Підтвердити | Potwierdź | Bestätigen | Confirmer | Confirmar | Onayla | 確認 | 확인 (a step or a winner: 확정) | 確定 | game (zh), proposed (pt, ja) | Core · Keyed/Dialogs_Various.xml · Confirm. The pt-BR game term is `Aceitar`, which collides with "accept" below; a confirmation before a write needs its own verb. Other locales: ja: the game's `了承` was rejected as too weak before a write. ko: 확정 for confirming a step or a merge winner, because 확인 means "check" in the same sentences; 확인 stays on the OK button. |
| accept | 接受 | aceitar | Принять | Прийняти | Akceptuj | Akzeptieren | Accepter | Aceptar | Kabul et | 受諾 | 수락 | 接受 | game | Core · Keyed/Dialogs_Various.xml · Accept |
| delete | 删除 | excluir | Удалить | Видалити | Usuń | Löschen | Supprimer | Borrar | Sil | 削除 | 삭제 | 刪除 | game | Core · Keyed/Menus_Main.xml · Delete |
| remove | 移除 | remover | Убрать | Прибрати | Usuń | Entfernen | Retirer | Quitar | Kaldır | 削除 | 제거 | 移除 | game (proposed in fr, es) | Core · Keyed/FloatMenu.xml · Remove. Other locales: fr: the game uses `Supprimer` for both Remove and Delete; `Retirer` keeps `Supprimer` as the strong verb. uk: `Видалити` where a removal destroys rows (an assignment section). es: the catalogue writes `Quitar`; the translator's note names the game's `Eliminar` (FloatMenu · Remove), so this is a deviation to review. pl: "Remove {mod}" reads `Usuń … z listy aktywnych`, never a bare `Usuń`. |
| remove (a mod from the active list) | 移除 | remover … da lista de mods ativos | убрать … из списка активных | прибрати з активного списку | usuń … z listy aktywnych | aus der aktiven Liste entfernen (button: Aus aktiver Liste entfernen) | retirer de la liste des mods actifs | quitar … de la lista de activos | etkin listeden kaldır | （有効なMODリストから）外す | 활성 목록에서 제거 | 移除 | proposed | `Action::RemoveMod` only takes a mod off the active list; a delete verb (ja 削除, zh 删除/刪除) reads as deleting the mod's files. ja keeps 削除 for Rimmerge's own data (a tag, a section, a scope member, a field). |
| add | 添加 | adicionar | Добавить | Додати | Dodaj | Hinzufügen | Ajouter | Añadir | Ekle | 追加 | 추가 | 新增 | game | Core · Keyed/Dialogs_Various.xml · Add |
| edit | 编辑 | editar | Изменить | Редагувати | Edytuj | Bearbeiten | Modifier | Editar | Düzenle | 編集 | 편집 (game: 수정) | 編輯 | game | Core · Keyed/Misc.xml · Edit |
| copy | 复制 | copiar | Копировать | Копіювати | Kopiuj | Kopieren | Copier | Copiar | Kopyala | コピー | 복사 | 複製 | game | Core · Keyed/MainTabs.xml · Copy |
| rename | 重命名 | renomear |  |  |  |  | Renommer | Renombrar | Yeniden adlandır |  | 이름 변경 | 重新命名 | game | Core · Keyed/Misc_Gameplay.xml · Rename |
| reset | 重置 | redefinir | Сбросить | Скинути | Resetuj | Zurücksetzen | Réinitialiser | Restablecer | Sıfırla | リセット | 초기화 | 重設 | game | Core · Keyed/Dialogs_Various.xml · ResetAll |
| revert | 恢复 | reverter | Откатить | Повернути | Cofnij | Zurücknehmen | Révoquer | Revertir | Geri al | 元に戻す | 되돌리기 | 還原 | game (proposed in de, fr) | Core · Keyed/Dialogs_Various.xml · ResolutionRevert. Other locales: zh-CN: "revert the decision" is 恢复为未决定, never 恢复决定. |
| overwrite | 覆盖 | substituir (button), sobrescrever (in a sentence) | Перезаписать | Перезаписати | Nadpisz | Überschreiben | Écraser | Sobreescribir | Üstüne yaz | 上書き | 덮어쓰기 | 覆蓋 (button), 覆寫 | game | Core · Keyed/Dialogs_Various.xml · OverwriteButton; Core · Keyed/Menu_KeyBindings.xml · KeyBindingOverwritten (`sobrescrito`) |
| restart | 重启 | reiniciar |  |  |  | Neustart | Redémarrer | Reiniciar (in-progress state: Reiniciando) |  |  | 재시작 |  | game (proposed in fr, es) | Core · Keyed/Dialogs_Various.xml · Restarting. Other locales: fr: the game's `Redémarrage` is a noun; a button takes the infinitive. |
| load (verb), loading | 载入, 载入中 | carregar, carregando | загрузка | завантаження | wczytaj, wczytywanie | laden | charger, chargement | cargar, cargando | yükle, yükleniyor | 読み込む | 불러오기, 불러오는 중 | 載入, 載入中 | game | Core · Keyed/Menus_Main.xml · Load; Core · Keyed/Dialogs_Various.xml · LoadingLongEvent. Other locales: This is the app loading a file or project. Three locales split it from the verb for a mod loading in the game: pl `ładować` (mods) / `wczytać` (app), ja `ロード` / `読み込む`, ko `로드` / `불러오기`. |
| downloading | 下载中 | baixando | загружать | завантажувати | pobierać | abrufen | téléchargement | descargando | indirme | 取得 | 내려받기 | 下載中 | game | Core · Keyed/Menus_Main.xml · Downloading |
| back | 返回 | voltar | Назад | Назад | Wróć | Zurück | Revenir | Volver | Geri dön | 戻る | 돌아가기 (game: 이전) | 返回 | game | Core · Keyed/Menus_Main.xml · Back. Other locales: es: the game's `Atrás` is not used for the button. |
| yes / no | 是 / 否 | sim / não | да / нет | так / ні | tak / nie | ja / nein | oui / non | sí / no | evet / hayır |  | 예 / 아니요 | 是 / 否 | game | Core · Keyed/Misc.xml · Yes, No. Other locales: ko: the game writes `아니오`; the standard spelling `아니요` is used. |
| settings | 设置 | configurações | Настройки | Налаштування | Ustawienia | Einstellungen | Paramètres | Ajustes | Ayarlar | 設定 | 설정 | 設定 | game | Core · Keyed/Menu_Options.xml · ModSettings; Core · Keyed/Menus_Main.xml · AdvancedSettings |
| options | 选项 | opções |  |  |  | Optionen | Options | Opciones | Ayarlar |  |  | 選項 | game | Core · Keyed/Menus_Main.xml · Options |
| details | 详细 | detalhes | подробности | подробиці | szczegóły | Details | détails | detalles | ayrıntılar | 詳細 | 세부 정보 (game: 상세) | 詳細資訊 (game: 細項) | game | Core · Keyed/Dialogs_Various.xml · Details |
| description | 描述 | descrição | Описание | Опис | Opis | Beschreibung | Description | Descripción | Açıklama | 説明 | 설명 | 描述 | game | Core · Keyed/Misc_Gameplay.xml · Description |
| filter | 筛选 | filtro | Фильтр | Фільтр | Filtr | Filter | Filtre | Filtro | Filtre | フィルター | 필터 | 篩選 | game | Core · Keyed/Misc_Gameplay.xml · Filter |
| search | 搜索 | pesquisar | поиск | пошук | szukaj | suchen | rechercher | buscar | ara | 検索 | 검색 | 搜尋 | game (zh), proposed (pt) | Core · Keyed/Dialogs_Various.xml · Searching. The pt-BR game term is `Procurando` (in-world searching); `pesquisar` is the usual verb for a search box. |
| priority | 优先级 | prioridade | Приоритет | Пріоритет | Priorytet | Priorität | Priorité | Prioridad | Öncelik |  | 우선순위 | 優先度 | game | Core · Keyed/Misc_Gameplay.xml · Priority |
| overview | 概况 | visão geral |  |  |  | Übersicht | vue d'ensemble | resumen | genel bakış |  | 개요 | 概觀 | game | Core · Keyed/ITabs.xml · HealthOverview |
| suggested | 建议 | sugerido | Предлагаемый | Запропонований | Sugerowana | Vorgeschlagen | Suggéré | Sugerido | Önerilen | 提案 | 제안 | 建議 | game | Core · Keyed/Dialogs_Various.xml · Suggested |
| assign | 分配 | atribuir | Назначить | Призначити |  | Zuweisen | Assigner | Asignar | Ata |  | 할당 |  | game | Core · Keyed/MainTabs.xml · AssignButton |
| name | 名称 | nome | Название | Назва | Nazwa | Name | Nom | Nombre | Ad | 名前 | 이름 | 名稱 | proposed (zh), game (pt) | Ideology · Keyed/MainTabs.xml · Name (`Nome`). The zh-CN game term there is `名字`, a person-like name; 名称 is the usual word for an item's name. |

## Rimmerge's own terms (no game equivalent)

Every row here is `proposed`: the game has no concept to borrow from.
The reason column says why the word was picked.

| English | zh-CN | pt-BR | ru | uk | pl | de | fr | es-ES | tr | ja | ko | zh-TW | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| load order | 加载顺序 | ordem de carregamento | порядок загрузки | порядок завантаження | kolejność ładowania | Ladereihenfolge | ordre de chargement | orden de carga | yükleme sırası | ロード順 | 로드 순서 | 載入順序 | The game says "must be loaded after" (`加载`, `carregado`) and "the order of your mod list" (Core · Keyed/Dialogs_Various.xml · ModsMismatchOrderChanged) but has no noun for the order itself. Both proposals build on those verbs and match modding-community usage. |
| def | Def | Def | Def | Def | def (declines: defu) | Def (die Def) | def (fém.) | Def (masc.) | Def | Def | Def | Def | A RimWorld XML definition. Modders on every language's forums use the English word; a def name such as `ThingDef` is code and stays as written. |
| patch (an XML patch) | 补丁 | patch (masc.: `o patch`) | патч | патч | łatka | Patch (der) | patch (masc., pl. patchs) | parche | yama | パッチ | 패치 | 補丁 | The standard zh word for any patch. pt-BR modders keep the loanword; `correção` means a bug fix and would mislead. |
| compatibility patch | 兼容补丁 | patch de compatibilidade | патч совместимости | патч сумісності | łatka zgodności | Kompatibilitätspatch (nav: Kompat-Patches) | patch de compatibilité (nav: patchs de compat.) | parche de compatibilidad | uyumluluk yaması | 互換パッチ | 호환 패치 | 相容性補丁 (nav: 相容補丁) | Built from "patch" above. |
| compatibility mod (a mod that exists for compatibility) |  | mod de compatibilidade |  |  |  |  |  |  |  |  | 호환용 모드 |  | `merge.mod.summary*`. ko: never 호환 모드, which reads as "compatibility mode". |
| patch maker | 补丁制作器 | criador de patches | конструктор патчей | конструктор патчів | kreator łatek | Patch-Ersteller | créateur de patchs | creador de parches | yama oluşturucu | パッチメーカー | 패치 메이커 | 補丁製作器 | Names the tool, not an action. |
| merge (verb), merge (noun) | 合并 | mesclar, mesclagem | объединить, слияние | об'єднати, об'єднання | scalać, scalanie | zusammenführen, Zusammenführung | fusionner, fusion | fusionar, fusión | birleştirmek, birleştirme | マージ | 병합 | 合併 | pt-BR software uses `mesclar` for merging (branches, documents); `fundir` reads as smelting. |
| merge mod, merge output | 合并 Mod, 合并输出 | mod de mesclagem, saída da mesclagem | мод слияния, результат слияния (nav: Слияние) | мод об'єднання, результат об'єднання | mod scalający, wynik scalania | Merge-Mod, Merge-Ausgabe | mod de fusion, résultat de fusion | mod de fusión, salida de fusión | birleştirme modu, birleştirme çıktısı | マージMOD, マージ出力 | 병합 모드, 병합 결과 | 合併模組, 合併輸出 | Built from "merge" and "mod" above. |
| finding | 检查结果 | achado | замечание (not «находка») | знахідка | ustalenie | Befund | constat | hallazgo | bulgu | 検出結果 | 발견 사항 | 檢查結果 | A result of the analysis that may or may not be a problem, so neither 问题 nor `problema`. `achado` is the pt-BR term for an audit or analysis finding. |
| ledger | 冲突账本 | registro de conflitos | реестр (журнал is taken by log) | реєстр конфліктів | rejestr konfliktów | Konfliktregister | registre des conflits | libro de conflictos (registro is taken by log) | çakışma defteri | 競合台帳 | 충돌 대장 | 衝突帳本 | The persistent list of findings and decisions. zh 账本 ("ledger") plus 冲突 keeps it concrete. pt avoids `livro-razão`, which is accounting jargon. |
| decision | 决定 | decisão | решение | рішення | decyzja | Entscheidung | décision | decisión | karar | 決定 | 결정 | 決定 | Plain word; the user's choice on a finding. |
| suggestion | 建议 | sugestão |  | пропозиція | sugestia | Vorschlag | suggestion | sugerencia | öneri | 提案 | 제안 | 建議 | Matches "suggested" above. |
| confidence | 置信度 | confiança | уверенность | впевненість | pewność | Konfidenz | confiance | confianza | güven | 信頼度 | 신뢰도 | 信賴度 | The standard statistics term in both languages. |
| conflict | 冲突 | conflito | конфликт | конфлікт | konflikt (verb: być w konflikcie) | Konflikt | conflit | conflicto | çakışma | 競合 | 충돌 | 衝突 | The game uses these words for genes and memes (Biotech · Keyed/Dialogs_Various.xml · GenesConflict), not for mods, so the meaning here is Rimmerge's. |
| rule | 规则 | regra | правило | правило | reguła | Regel | règle | regla | kural | ルール | 규칙 | 規則 | Plain word. |
| pair rule | 成对规则 | regra de par | парное правило | парне правило | reguła pary | Paarregel | règle de paire | regla de par | ikili kural | ペアルール | 쌍 규칙 | 成對規則 | A rule about the order of two mods. |
| placement rule | 位置规则 | regra de posicionamento | правило позиции | правило розміщення | reguła położenia | Platzierungsregel | règle de placement | regla de posición | konum kuralı | 配置ルール | 배치 규칙 | 位置規則 | A rule pinning one mod's position. |
| rules database | 规则数据库 | banco de dados de regras | база правил | база правил | baza reguł | Regeldatenbank | base de règles | base de datos de reglas | kural veritabanı | ルールデータベース | 규칙 데이터베이스 | 規則資料庫 | Plain compound. |
| edge (a sort constraint between two mods) | 排序约束 | restrição de ordem | связь (fem.) | обмеження порядку | ograniczenie | Sortierbedingung | contrainte (fém.) | restricción de orden (fem.) | kısıt (sıralama kısıtı) | 順序制約 | 순서 제약 | 排序約束 | The graph word (边, `aresta`) means nothing to a player; the UI shows an edge as "A must load before B", which is an ordering constraint. |
| tier | 层级 | camada | ярус | ярус | warstwa | Schicht | tranche | capa | katman | 階層 | 계층 | 層級 | One of the sorter's bands (Core, DLC, Top, Body, Bottom). The game's growth-tier word (`级别`, `nível`, Biotech · Keyed/Dialogs_Various.xml · MaxTier) reads as a level or grade, which is not what a tier is here. |
| verify | 验证 | verificar | проверить | перевірити | weryfikuj | prüfen | vérifier | verificar | doğrula | 検証 | 검증 (the Apply dialog's Verify order button and heading: 순서 검증, quoted as '순서 검증' when prose names it) | 驗證 | Predicts what the game log would say about an order. |
| scan, rescan | 扫描, 重新扫描 | escanear, escanear novamente | сканировать, пересканировать | сканувати, пересканувати | skanuj, skanuj ponownie | scannen, erneut scannen | analyser, réanalyser | escanear, volver a escanear | tara, yeniden tara | スキャン, 再スキャン | 스캔, 다시 스캔 | 掃描, 重新掃描 | `verificar` is taken by "verify". |
| startup cost | 启动开销 | custo de inicialização | затраты на запуск | витрати на запуск | koszt uruchomienia | Startkosten | coût au démarrage | coste de arranque | açılış maliyeti | 起動コスト | 시작 비용 | 啟動耗時 | Time the game spends loading a mod at startup. |
| profile (Rimmerge's decision store) | 配置档案 | perfil | профиль | профіль | profil | Profil | profil | perfil | profil | プロファイル | 프로필 | 設定檔 | zh avoids 配置文件, which means "config file". Other locales: tw: 設定檔 is the established Taiwan term for a profile (Windows 使用者設定檔, Chrome 設定檔, Firefox 設定檔管理員); a config file is 設定檔案 (Microsoft: 組態檔), never bare 設定檔. |
| dashboard | 概览 | painel | сводка | зведення | panel | Dashboard | tableau de bord | panel | pano | ダッシュボード | 대시보드 | 總覽 | A summary page; 仪表盘 reads as a car dashboard. |
| current (order source) | 当前 | atual | текущий | поточний | bieżąca | aktuell | actuel | actual | mevcut | 現在 | 현재 | 目前 | Plain word, paired with "suggested". |
| preview (the language picker's label) | 预览 | prévia | предварительный | тестова версія | wersja wstępna | Vorschau | aperçu | preliminar | önizleme | プレビュー | 미리 보기 | 預覽版 | Marks a locale as not yet reviewed by a native speaker. Other locales: uk: `попередня версія` reads as "previous version". |
| override (a rule that changes an outcome) | 覆盖 | sobrepor / sobreposição | переопределить | перевизначити | nadpisać | übersteuern | outrepasser (verb), dérogation (noun, user rule) | invalidar (declaration), reemplazar (def, texture, sound) | geçersiz kılmak | 上書き | 재정의 | 覆寫 | The verb for a pair/placement rule taking effect over the sorter's own default. |
| overruled (a finding whose declared relation lost to a stronger one) | 否决 | anulado(a) | отвергнуто | відхилено | uchylone | überstimmt | rejeté | anulada | bastırıldı | 否決 | 기각됨 | 否決 | `inbox.evidence.ruleOverruled`/`placementOverruled`'s own past-tense verb. |
| questioned (a finding whose placement conflicts with an accepted edge) | 受到质疑 | questionado(a) | под сомнением | під сумнівом | podważone | infrage gestellt | contesté | cuestionada | sorgulanıyor | 疑義 | 이의 제기됨 | 受到質疑 | `inbox.evidence.placementQuestioned`'s own past-tense verb — weaker than "overruled": the placement still held, but evidence pushes against it. |
| dropped edge (an edge the sorter gave up rather than honor) | 舍弃 | restrição descartada | отброшена | відкинуто | porzucone | aufgegeben | abandonnée | descartada | atıldı | 破棄 | 포기 | 捨棄 | `inbox.evidence.edgeDropped`'s own verb. |
| drop (a merge value discarded during a merge) | 丢弃 | descartar / descartado | исключить | вилучити | odrzucić | verwerfen | écarter | descartar | atmak | 破棄 | 버리기 | 丟棄 | Distinct from "dropped edge" above — a merge-evaluator outcome, not a sort one. |
| winner (the edge, rule, or value that took effect) | 胜出 / 胜出者 | vencedor | победитель | переможець | zwycięzca | Gewinner | gagnant | ganador | kazanan | 優先(される側) | 승자 | 勝出者 | 胜出 as a verb ("prevails"), 胜出者 as the noun ("the winner"). |
| owner (the mod responsible for a patch, asset, or def) | 所有者 | dono | владелец | власник | właściciel | Eigentümer | propriétaire | dueño | sahip | 所有MOD | 소유자 | 所有者 |  |
| patcher (a mod that applies a runtime patch) | 修补者 | aplicador de patch (role label: aplica patch) | патчер | патчер | łatający | Patcher | patcheur | parcheador | yamalayan | パッチ (role label) | 패치 적용자 | 修補者 | `inbox.evidence.runtimePatchCollision`'s own noun for each patching mod. |
| edge strength: Hard / Declared / Soft / Awareness / Inferred | 硬性 / 声明 / 软性 / 感知 / 推断 | Rígida / Declarada / Suave / Percepção / Inferida | Жёсткая / Объявленная / Мягкая / Сигнальная / Выведенная | Жорстка / Задекларована / М'яка / Обізнаність / Виведена | Twarda / Deklarowana / Miękka / Świadomość / Wywnioskowana | Hart / Deklariert / Weich / Kenntnis / Abgeleitet | Stricte / Déclarée / Souple / Perception / Déduite | Rígida / Declarada / Flexible / Percepción / Inferida | Kesin / Beyan / Yumuşak / Farkındalık / Çıkarım | ハード / 宣言 / ソフト / 認識 / 推論 | 하드 / 선언 / 소프트 / 인지 / 추론 | 硬性 / 宣告 / 軟性 / 感知 / 推斷 | `utils/edgeStrength.ts`'s five labels — the sort-evidence tiers `docs/concepts/sorting.md` names. |
| any-of (a placement rule any one candidate can satisfy) | 任选其一 | alternativas | одно из | будь-який з | jeden z wielu | Eine von | au choix | alternativas | herhangi biri | いずれか | 택일 | 任選其一 | `layer.anyOf`. Other locales: de: compounds hyphenate: `Eine-von-Auswahl`, `Eine-von-Kandidat`. |
| pinned (a mod fixed to a placement) | 固定 | fixado | закреплён | закріплено | przypięty | fixiert | fixé | fijado | sabitlenmiş | 固定 | 고정 | 固定 |  |
| placement: Top / Bottom | 顶部 / 底部 | Topo / Fundo | Вверху / Внизу | Верх / Низ | Góra / Dół | Oben / Unten | Haut / Bas | Superior / Inferior | Üst / Alt | 先頭 / 末尾 | 상단 / 하단 | 頂部 / 底部 | `utils/placement.ts`'s two labels — a specific mod's own pinned position, distinct from a sort tier band (next row). |
| tier band: Body | 主体 | Corpo | Середина | Основа | Środek | Mitte | Milieu | Medio | Gövde | 本体 | 본체 | 主體 | `utils/tier.ts`'s middle band — Core/DLC/Top/Body/Bottom; only Body needed a proposal, the others already read as plain words. |
| ship (a mod bundling an asset) | 打包 | incluir | поставлять (action: взять в мод слияния, `Взять этот файл`) | постачати |  | mitliefern | fournir | incluir | barındırmak (action: dahil et) | 同梱 | 포함 | 打包 / 提供 | `inbox.evidence.duplicateAssembly`/`soundOverride`'s own verb. Other locales: ru: the action is never `включить`, which is "activate". |
| shipped by (which mod provides an asset) | 提供 | incluído por | поставляют | яку постачають | dostarczana przez | mitgeliefert von | fourni par | incluido en | barındıranlar | 同梱しているMOD | 포함한 모드 | 打包於 |  |
| shipped asset (a decision to include an asset in the merge mod) |  | recurso incluído |  |  |  |  |  |  | dahil edilen varlık (kararı) |  |  |  | Builds on the "ship" action. tr: kept apart from `barındırmak` (a mod providing an asset); `varlık` also means "presence", so keep it out of sentences about a mod being present. |
| asset (a texture, sound, or other shipped file) | 资源 | recurso | ресурс | ресурс | zasób | Asset | ressource | recurso | varlık | アセット | 에셋 | 資源 |  |
| section (an assignment project's own subdivision) | 分区 | seção | раздел | розділ | sekcja | Abschnitt | section | sección | bölüm | セクション | 섹션 | 分區 | `assignments.editor`'s own term for one coverage/row-editor scope within a project. |
| coverage (how much of an assignment's target set has a decision) | 涵盖 | cobertura (row state: sem cobertura) | покрытие | охоплення | pokrycie | Abdeckung | couverture | cobertura | kapsam | カバー状況 (counted: 対応済み / 未対応) | 적용 범위 | 涵蓋 |  |
| schema (an assignment type's inferred field shape) | 结构 | esquema | схема | схема | struktura | Schema | schéma | estructura | şema | スキーマ | 스키마 | 結構 |  |
| assignment type | 分配类型 | tipo de atribuição |  | тип призначення | typ przypisania | Zuweisungstyp | type d'attribution | tipo de asignación | atama türü |  | 할당 유형 | 分配類型 | The kind of thing an assignment project produces (a patch-maker concept). |
| assignment (the patch-maker's own project) | — | atribuição | назначение | призначення | przypisanie | Zuweisung | attribution | asignación (generated-mod label: proyecto de asignación) | atama | 割り当て | 할당 | 分配專案 |  |
| orphaned decision (a decision whose target no longer exists) | 孤立决定 | decisão órfã | осиротевший | осиротілий | osierocona decyzja | verwaist | orpheline | huérfana | sahipsiz karar | 孤立した | 대상 없는 결정 | 孤立決定 | `patches.detail.orphanedCount`/`patches.scopeEditor`'s own noun. |
| prune (discard orphaned decisions) | 清理 | limpar | очистить | очистити | wyczyść | bereinigen | purger | depurar | temizle | 整理 | 정리 | 清理 |  |
| Keyed translation collision | 翻译键冲突 | colisão de chaves de tradução | Конфликт ключей перевода | Колізія ключів перекладу | Kolizja kluczy tłumaczeń Keyed | Keyed-Übersetzungskollision | Collision de traductions Keyed | Colisión de traducción Keyed | Keyed çeviri çakışması | Keyed翻訳キーの衝突 | Keyed 번역 키 충돌 | 翻譯鍵衝突 | `inbox.evidence.keyedTranslationCollision`'s own finding name. |
| transpiler (a Harmony runtime-patch kind) | 转译器 | transpiler (masc.) | транспайлер | транспілер | transpiler | Transpiler | transpiler | transpilador | transpiler | トランスパイラー | 트랜스파일러 | 轉譯器 | `inbox.evidence.transpilerCollision`. |
| runtime patch | 运行时补丁 | patch em tempo de execução | патч времени выполнения | патч під час виконання | łatka w czasie działania | Laufzeit-Patch | patch d'exécution | parche en tiempo de ejecución | çalışma zamanı yaması | ランタイムパッチ | 런타임 패치 | 執行階段補丁 | A Harmony patch applied at runtime, as opposed to an XML `PatchOperation`. |
| source: DLC | 扩展 | Expansão | DLC | розширення | DLC | DLC | extension | expansión | DLC | DLC | 확장팩 | 擴充 | `SourceDto`'s own DLC value, reusing "expansion" from the "Where mods come from" table above. Other locales: ko: 확장팩 here and on the sorter tier badge (`order.tier.dlc`) alike. |
| needs input (a decision status) | 待处理 | requer decisão | требует решения | потребує рішення | wymaga decyzji | Entscheidung nötig | décision requise | requiere decisión | karar bekliyor | 要判断 | 판단 필요 | 待處理 | A finding whose confidence falls below the auto-decide threshold. |
| overridden (a decision status set by a manual choice) | 手动决定 | sobreposto | переопределено | перевизначено | zmienione ręcznie | Übersteuert | choix manuel | decisión manual | elle belirlendi | 手動で決定 | 수동 결정 | 手動決定 | Distinct from "needs input" — the user (or a rule) already decided against the suggestion. |
| tie-break | 并列处理 | desempate | порядок при равенстве | розв'язання нічиїх | rozstrzyganie remisów | Bei Gleichstand | départage | desempate | eşitlik bozma | 同順位の処理 | 동순위 처리 | 並列處理 | `settings.tieBreak`'s own concept — how equally-ranked mods are ordered against each other. |
| Details (an expandable section's own toggle label) | 详细信息 | Detalhes |  |  |  |  |  |  |  |  |  | 詳細資訊 | Longer than the plain noun "details" (已有 详细) — a button/toggle label reads better as a full phrase than the bare noun. |
| scan (noun, e.g. "the last scan") | — | escaneamento |  | сканування | skanowanie | Scan | analyse | escaneo | tarama |  |  | 掃描 | Distinct from the existing verb pair `escanear`/`escanear novamente` above — a noun is needed where the UI names the scan itself, not the action. |
| template (a def registered for other defs to inherit from) | — | modelo | шаблон | шаблон | szablon | Vorlage | modèle | plantilla | şablon | テンプレート | 템플릿 | 範本 |  |
| replay (re-running a patch stack to verify it) | — | reprodução | воспроизведение | відтворення | odtwarzanie | Wiedergabe (erneut abspielen) | rejeu | reproducción | yeniden oynatma | リプレイ | 재생 | 重播 | `docs/concepts/verify.md`'s own mechanism. |
| assembly (a .NET DLL) | — | assembly (loanword, masc.: `o assembly`) | сборка | збірка | biblioteka DLL | Assembly | assembly (masc.) | ensamblado | assembly | アセンブリ | 어셈블리 | 組件 | Modders' own term; a translation would be unrecognizable. |
| tag (an inferred mod category) | — | tag (loanword, fem.: `a tag`) | тег | тег | tag | Tag | tag (masc.) | etiqueta | etiket | タグ | 태그 | 標籤 | `inbox.evidence.tagInferred`'s own noun. |
| game log (an imported `Player.log` or console snapshot) | 游戏日志 | registro do jogo | журнал игры | журнал гри | log gry | Spiel-Log | journal du jeu | registro del juego | oyun günlüğü | ゲームログ | 게임 로그 | 遊戲日誌 | `dashboard.importLog.*`; builds on log = 日志 / `registro`. The file name `Player.log` stays literal. |
| console snapshot (a copy of the in-game debug console) | 控制台快照 | captura do console | снимок консоли | знімок консолі | zrzut konsoli | Konsolen-Schnappschuss | capture de la console | captura de la consola | konsol anlık görüntüsü | コンソールのスナップショット | 콘솔 스냅샷 | 主控台快照 | `gameLog.kind.consoleSnapshot`. "Console" is the modding community's word for the debug log window; pt `instantâneo` reads oddly. |
| log entry | 记录 (counted: 条记录) | entrada |  | запис | wpis | Eintrag | entrée | entrada | kayıt | エントリ | 항목 | 訊息 (counted: 則) | One message with its stack trace. zh keeps it distinct from 条目, used for list and map entries in the merge UI. |
| family of entries | 类记录 | família de entradas |  | сімейство записів | rodzina wpisów | Gruppe von Einträgen | famille d'entrées | familia de entradas | kayıt ailesi | エントリ群 | 항목 그룹 | 類訊息 | Repeated entries grouped under one key. |
| startup pass | 启动轮次 (counted: 轮启动) | inicialização | запуск | запуск | uruchomienie | Startdurchlauf | démarrage | arranque | açılış turu | 起動 (counted: 回) | 시작 회차 | 輪啟動 | The game starts twice when a pre-patching loader restarts it. pt `passagem de inicialização` is unidiomatic, and `inicialização` matches "startup cost". |
| clean exit / exit footer | 正常退出 / 退出结尾 | saída limpa / rodapé de saída | штатный выход | штатний вихід / завершальний блок виходу | czyste wyjście / stopka wyjścia | sauberes Beenden / Abschlusszeilen | sortie propre / message de fermeture | salida limpia / pie de salida | temiz çıkış / çıkış kaydı | 正常終了 / 終了の記録 | 정상 종료 / 종료 꼬리말 | 正常退出 / 退出結尾 | How a whole `Player.log` ends. |
| crash (the game) / crash report | 游戏崩溃 / 崩溃报告 | encerrar com falha / relatório de falha | вылет игры / отчёт о сбое | аварійне завершення гри / звіт про збій | awaria / raport awarii (freeze: zawieszenie) | Absturz | plantage / rapport de plantage | cierre inesperado / informe de fallo | çökme / çökme raporu | クラッシュ / クラッシュレポート | 크래시 | 當機 / 當機報告 | pt avoids `travar`, which means "freeze"; a freeze is its own case in the cut-off line. |
| failure (a patch operation failing) | 失败 | falha | сбой | збій | niepowodzenie | Fehler / fehlschlagen | échec | fallo | başarısız işlem |  | 실패 | 失敗 | A patch operation that fails while the game keeps running. ru: `сбой` here, `вылет игры` for a crash; `отчёт о сбое` stays the crash report. Other locales: pt: a crash stays `encerrar com falha`. ko: a crash is 크래시. |
| a log cut off mid-run | 日志在运行中途结束 | o registro para no meio da execução |  | журнал обривається посеред роботи | urywa się | bricht mitten im Lauf ab | le journal s'arrête en pleine exécution | se corta a mitad de ejecución |  |  | 실행 도중에 끝남 | 日誌在執行中途結束 | Never the logging-gap word below: a cut-off log and a logging gap are different events. |
| logging gap (the game stopped writing messages) | 日志中断 | lacuna de registro (stop: parada de registro; verb: registrar) | перерыв в записи | пропуск журналювання (stop: зупинка журналювання) | luka w logowaniu | Protokollierungslücke | interruption de la journalisation | interrupción del registro | günlük kesintisi (after a number: bare `kesinti`, `{count} kesinti`) | ログ記録の中断 | 로그 기록 중단 | 日誌中斷 | After 10,000 lines the game stops logging until reset. It is the logging that resumes, not the gap (pt `o registro de mensagens foi retomado`). Other locales: tr: `{count} günlük kesintisi` reads as "an N-day gap", so a count takes the bare `kesinti`. |
| lower bound (a count that is a minimum) | 下限 | valor mínimo | минимальное значение | мінімальне значення (adj.: занижений) | wartość minimalna | Mindestwert | valeur minimale | valor mínimo | en az değer | 下限値 | 하한 | 下限 | "The count may be short." pt `limite inferior` is reserved for the sorter's placement bound (`order.whyPanel.lowerBoundsHeading`) and is never used for counts. Other locales: The sorter's own bounds are the next row. |
| lower bound / upper bound (the sorter's, `order.whyPanel.*`) | 下界 / 上界 | limite inferior | нижние / верхние границы | нижні межі / верхні межі | dolne granice |  | borne inférieure | límites inferiores / superiores |  | 下限 / 上限 | 선행 모드 / 후행 모드 | 下界 / 上界 | A mod set, not a number: the mods this mod must load after, and the mods waiting on it. ko: never 하한/상한, which are the count's minimum and the cap. ja: 下限 here, 下限値 for a count. Other locales: zh: 下界/上界 here, 下限 for a count. ru: `минимальное значение` for a count. |
| cap (the console's 1,000-entry limit) | 上限 | limite |  | ліміт | limit | Obergrenze | limite | límite (de la consola) | konsol sınırı | 上限 | 상한 | 上限 |  |
| one game run (as in "not a whole session") | 整次游戏运行 | execução do jogo | сессия (игры) | ігровий сеанс | działanie gry (zapis całego działania gry) | Spiellauf | exécution du jeu | ejecución completa del juego | oyun çalışması | 1回のゲーム実行 | 게임 실행 | 整次遊戲執行 | Never the word the locale uses for the app's own session (`error.code.session_lost`): zh 会话, pt `sessão`, pl `sesja`, de `Sitzung`, fr `session`, es `sesión`, tr `oturum`, ko 세션. ru `сессия` is safe because the app's session is `сеанс`; uk qualifies `сеанс` as `ігровий сеанс`. |
| stack trace / back-reference | 堆栈跟踪 / 反向引用 | rastreamento de pilha / referência retroativa de pilha |  | трасування стека / зворотне посилання | ślad stosu / odwołanie wsteczne | Stacktrace | trace de pile / référence arrière de pile | traza de pila / referencia retroactiva | yığın izi | スタックトレース / 後方参照 | 스택 추적 / 스택 역참조 | 堆疊追蹤 |  |
| reader (the log reader) | 读取器 | leitor | модуль чтения | зчитувач | czytnik | Log-Leser | lecteur | lector | okuyucu | 読み取り処理 | 로그 판독기 | 讀取器 | The part of Rimmerge that reads the log file. |
| "predicted, not observed" | 已预测，但未观察到 | previsto, não observado | предсказано, не наблюдалось | передбачено, не спостережено | przewidywane, niezaobserwowane | vorhergesagt, nicht beobachtet | prévu, non observé | previsto, no observado | öngörüldü, gözlenmedi | 予測されたが未観測 | 예측됨, 관찰되지 않음 | 已預測，但未觀察到 | The quoted phrase in the Apply dialog's notes; keep it identical in every note. |
| lazy (a reference resolved only when read, not at scan time) | — | tardia | отложенная | відкладено розв'язуваний | leniwe (odwołanie) | verzögert | différé | diferida | geç (geç başvuru, geç çözümlenen) | 遅延 | 지연 | 延遲（參照） | `edgeKind.mayRequire`-style "lazy reference" concept. |
| internet access (the network switch) | 互联网访问 | acesso à internet | Доступ в интернет | Доступ до інтернету | dostęp do internetu | Internetzugriff | accès à Internet | acceso a internet | internet erişimi | インターネットアクセス | 인터넷 접속 | 網際網路存取 | `settings.network.*`. Never the bare word for "network" (zh-CN 网络, pt `rede`): a user reads that as their local network, while the app only ever contacts two GitHub hosts. |
| counterfactual (re-checking a failure under other load orders) | 反事实分析 | contrafactual | Контрфактическая проверка | Контрфактичний аналіз | analiza kontrfaktyczna | Gegenprobe | contrefactuel | contrafactual | karşı olgusal analiz | 反実仮想 | 반사실 검사 | 反事實檢驗 | `apply.diff.counterfactualSummary`; `docs/concepts/verify.md`'s phase that retries a failing operation under other orders. |
| collision (two mods defining the same patch target, translation key or assembly) | 冲突 | colisão | конфликт | колізія | kolizja | Kollision | collision | colisión | çakışma | 衝突 | 충돌 | 衝突 | `finding.kind.*Collision`. Kept apart from "conflict" in uk, pl, de, fr, es and pt-BR; ru, tr and the Chinese locales use the same word for both. Other locales: pl: the verb for conflict is `być w konflikcie`; `kolidować` only for collisions. |
| promote (a rule to a user decision) | 提升 | promover | сделать своим | перенести в рішення | przejąć | hochstufen | promouvoir | promover | yükseltmek | 昇格 | 승격 | 提升 | `rules.pairTable.promoteButton`. ru and uk reword it as an action ("make it yours", "move into decisions") because a bare verb reads as a rank change. |
| patch scope | 范围 | escopo | область | область дії | zakres | Geltungsbereich | portée | ámbito | kapsam | 範囲 | 범위 | 範圍 | The set of mods a compatibility patch covers (`patches.list.scopeHeading`). |
| reference mods (R) / targets (T) | 引用 Mod (R) / 目标 (T) | mods de referência (R) / alvos (T) | справочные моды (R) / цели (T) | моди-посилання (R) / цілі (T) | mody referencyjne (R) / cele (T) | Referenz-Mods (R) / Ziele (T) | mods de référence (R) / cibles (T) | mods de referencia (R) / objetivos (T) | başvuru modları (R) / hedefler (T) | 参照MOD（R） / 対象（T） | 참조 모드 (R) / 대상 (T) | 參照模組（R） / 目標（T） | The patch maker's two mod sets. The letters R and T stay as written in every locale. |
| facing (a texture's direction) | 朝向 | direção | направление | напрямок | kierunek | Blickrichtung | orientation | orientación | yön | 向き | 방향 | 朝向 | `defGraphics.viewer.facingGroupLabel`; the short N/E/S/W letters follow each language's own compass letters. |
| variant (one of a def's textures) | 变体 | variante | вариант | варіант | wariant | Variante | variante | variante | varyant | バリエーション | 변형 | 變體 | `defGraphics.viewer.variantSelectLabel`. |
| texture source (where a def's texture comes from) | 纹理来源 | origem da textura | источник текстуры | джерело текстури | źródło tekstury | Texturquelle | source de la texture | origen de la textura | doku kaynağı | テクスチャの提供元 | 텍스처 출처 | 紋理來源 | `defGraphics.viewer.slotSelectLabel`. |
| life stage / body type / head type | 生命阶段 / 体型 / 头部类型 | estágio de vida / tipo de corpo / tipo de cabeça | стадия жизни / тип тела / тип головы | етап життя / тип тіла / тип голови | etap życia / typ ciała / typ głowy | Altersgruppe / Körpertyp / Kopftyp | stade de vie / type de corps / type de tête | etapa de vida / tipo de cuerpo / tipo de cabeza | yaşam evresi / vücut tipi / kafa tipi | ライフステージ / 体型 / 頭の種類 | 생애 단계 / 체형 / 머리 유형 | 生命階段 / 體型 / 頭型 | `defGraphics.slot.*` and `defGraphics.variant.*`: the pawn graphics a def can switch between. |
| pawn kind | 角色种类 | tipo de criatura | тип пешки | тип істоти | rodzaj postaci | Figurenart | type de personnage | tipo de personaje | canlı türü | ポーンの種類 | 폰 종류 | 角色類型 | `defGraphics.slot.raceKind`. "Pawn" itself has no shared term: ja and ko keep the loanword, pt-BR uses `personagem` for a humanlike pawn. |
| worn (a texture shown on a pawn) | 穿戴 | no corpo | надетое | одягнене | na postaci | getragen | porté | puesto | giyili | 着用時 | 착용 시 | 穿戴 | `defGraphics.slot.worn`. |
| mirrored (a texture drawn flipped) | 镜像绘制 | exibido espelhado | отображается зеркально | віддзеркалено | rysowana w lustrzanym odbiciu | gespiegelt gezeichnet | dessinée en miroir | dibujada en espejo | aynalanmış çizilir | 反転して描画 | 좌우 반전하여 그림 | 鏡像繪製 | `defGraphics.viewer.mirrored`. |
| precedence rule | 优先规则 | regra de precedência | правило приоритета | правило пріоритету | reguła pierwszeństwa | Vorrangregel | règle de priorité | regla de precedencia | öncelik kuralı | 優先ルール | 우선순위 규칙 | 優先規則 | `ruleWarning.what.precedenceRule`: a rules-database entry that ranks which framework wins. |
| gate (a condition under which a patch applies) | 条件 | condição | условие | умова | warunek | Bedingung | condition | condición | koşul | 条件 | 조건 | 條件 | `defPage.gatesPrefix`. |
| cluster (a rules-database group of mods) | 集群 | grupo | кластер | кластер | klaster | Cluster | groupe | clúster | küme | クラスター | 클러스터 | 群組 | `action.excludeFromCluster`. |
| technical details | 技术细节 | detalhes técnicos | технические подробности | технічні подробиці | szczegóły techniczne | technische Details | détails techniques | detalles técnicos | teknik ayrıntılar | 技術的な詳細 | 기술 세부 정보 | 技術細節 | `common.technicalDetail`: the English error text shown under a translated message. |
| apply anyway / write anyway | 仍然应用 / 仍然写入 | aplicar mesmo assim / escrever mesmo assim (write a file: escrever, never gravar) | всё равно применить / всё равно записать | все одно застосувати / все одно записати | zastosuj mimo to / zapisz mimo to | trotzdem anwenden / trotzdem schreiben | appliquer malgré tout / écrire malgré tout | aplicar de todos modos / escribir de todos modos | yine de uygula / yine de yaz | それでも適用 / それでも書き込む | 그래도 적용 / 그래도 기록 | 仍要套用 / 仍要寫入 | The confirmation steps in the Apply dialog; each stays as strong as the English. |
| rebuild / preserve current (the tie-break choices) | 重建 / 保留当前顺序 | reconstruir / preservar atual | перестроить / сохранить текущий | перебудувати / зберегти поточний | odbuduj / zachowaj bieżącą | neu aufbauen / aktuelle beibehalten | reconstruire / conserver l'actuel | reconstruir / conservar el actual | yeniden kur / mevcudu koru | 再構築 / 現在を維持 | 재구성 / 현재 유지 | 重建 / 保留目前 | `settings.tieBreak.*`; see `docs/concepts/sorting.md`. |
| internet-facing wording: "refresh", "download", "fetch" | 刷新 / 下载 / 获取 | atualizar / baixar / buscar | обновить / загрузить / получить | оновити / завантажити / отримати | odśwież / pobrać / pobrać | aktualisieren / herunterladen / abrufen | actualiser / télécharger / récupérer | actualizar / descargar / descargar | yenile / indir / al | 更新 / ダウンロード / 取得 | 새로 고침 / 내려받기 / 내려받기 | 重新整理 / 下載 / 下載 | Rule-database refresh versus the app loading a file: the two never share a verb (see "load" above). Other locales: ko: 가져오기 is Import (RimSort import, game-log import, rule re-import), never a download. |
| check for updates (the release check) | 检查更新 | verificar atualizações | Проверять обновления |  | Sprawdzaj aktualizacje |  | Rechercher des mises à jour | buscar actualizaciones |  | アップデート（の確認） | 업데이트 확인 | 檢查更新 | The `api.github.com` release check. ja: アップデート, never 更新, which is the rule-database refresh above; the two never share a word. Other locales: zh: 检查更新, never 刷新 (the refresh). |
| dismiss (a notice) | 关闭 | dispensar | Скрыть | Сховати | Ukryj | Ausblenden | Masquer | Descartar |  |  | 닫기 | 關閉 | Hides a notice and discards nothing. de: kept apart from `verwerfen`, the "drop" verb. |
| cosmetic (no functional difference) | 无实质影响 | cosmético | косметика | косметичний | kosmetyczny |  | cosmétique | estético |  |  | 표면적 (차이) | 無實質影響 | The final defs are identical and only a log line differs. zh: 外观/外觀 means visual appearance and stays only for a texture (`rationale.textureOverrideCosmetic`: 只影响外观 / 只影響外觀). |
| pending (an activation change no rescan has picked up) | 待定 (待定更改, 待定的启用状态更改) | pendente | ожидающий |  | oczekujący | ausstehend | en attente | pendiente |  |  | 대기 중인 | 尚未掃描 (尚未掃描的變更; alternative: 待定) | zh: 待处理/待處理 is reserved for "needs input"; on an export's data-loss warning the two must not read alike. |
| map entry (a keyed-list field row) | 映射条目 | entrada de dicionário | элемент таблицы (map: таблица) | елемент словника (map: словник) | wpis słownika (map: słownik) | Zuordnungseintrag (map: Zuordnung) | entrée de table | entrada de diccionario (map: diccionario) | map: eşleme |  | 키-값 항목 (map: 키-값 목록) | 對應表項目 | Never the word for the game map (`Map`, `mapa`, `карта`, `мапа`, `harita`, 맵, 地图): a player reads "replaced the whole map" as the colony map. uk: `мапа` is the game's colony map (Core · Keyed/Misc_Gameplay.xml). |
| framework mod / leaf mod |  | framework / mod folha |  |  |  |  |  | framework / mod final | altyapı modu / uç mod |  |  | 框架 / 末端模組 | A mod other mods build on, and a mod nothing depends on. |
| analyzer / sorter |  | analisador / ordenador |  | — / сортувальник |  |  |  | analizador / motor de ordenación | analizci / sıralayıcı |  |  | 分析器 / 排序器 | Rimmerge's two engines: the scan that extracts facts, and the load-order sort. |
| Not in effect (a rule or import that is off) |  |  |  |  |  |  | Pas en vigueur |  | Etkisiz | 未反映 | 효력 없음 |  | A state, not an action: never the Apply verb, so it cannot read as "not applied yet, click Apply". |
| display limit (a UI list truncated for display) |  |  | ограничение отображения |  |  | Anzeigelimit | limite d'affichage |  | görüntüleme sınırı | 表示上限 | 표시 한도 |  | Rimmerge shortening a list on screen. Distinct from "cap" (the console's own 1,000-entry limit, de `Obergrenze`). |
| origin (where a rule or tag comes from) |  |  |  |  |  |  |  |  |  | 由来 |  |  | The source a rule or an inferred tag was taken from (a rules database, a user decision, a scan). |

## Never translated

These stay exactly as written in every locale. Put them in `<code>` or
leave them as the placeholder that carries them:

- mod names, mod descriptions and author names (mod-authored);
- package ids and mod ids;
- def names, def types, `ParentName`s, xpaths, field paths and patch
  operation classes (`PatchOperationReplace`);
- file names and paths (`ModsConfig.xml`, `About.xml`,
  `LoadFolders.xml`, `Player.log`);
- sha prefixes;
- keyboard-shortcut letters;
- game log text;
- anything a user wrote: decision notes, rule comments, patch and
  assignment descriptions.
