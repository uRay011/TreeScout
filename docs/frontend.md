# TreeScout — フロントエンド（UI）設計

> 関連ドキュメント: [architecture.md](architecture.md)（全体構成・検索フロー） / [backend.md](backend.md)（Rustコア実装詳細） / [status.md](status.md)（フェーズ計画・リスク・参考リンク）

---

## 1. UIスタック

```
React 19 + TypeScript 5
├── Vite 6 (ビルドツール)
├── Tailwind CSS v4 (ユーティリティCSS)
├── Framer Motion (カラムスライドイン・ヒートマップアニメーション)
├── cmdk (コマンドパレットUI)
├── shadcn/ui (コンポーネント)
└── @tauri-apps/api (IPC通信 / A*探索ストリーミング受信)
```

---

## 2. デザイン方針：AIガイドパスライン＋ヒートマップ

A*探索が開いたパスをリアルタイムで可視化する探索型カラムUIを採用する。
AIが最高スコアのルートを「光るパス」として提示し、ユーザーは確認・逸脱が自由にできる。

**1カラム = 1フォルダの子要素一覧**（Millerカラム/Finderのカラム表示と同様）。
次カラムの内容は「直前カラム内でスコア（h_score）が最大のフォルダ」の子要素に限定される
——探索順序が先だった兄弟フォルダ（別年度のフォルダ等）の中身が混入してはならない。
スコアの低い兄弟フォルダ（`hooks/`・`utils/`等）はそのカラム内に薄い項目として見えるが、
次カラムへは展開されない。

```
クエリ: "Reactのボタンコンポーネント"

  src/          components/       Button/
  ████████  →   ████████████  →   █████████████   Button.tsx    ★0.97
                 ▓▓▓▓▓ hooks/                      styles.css
                 ░░░░░ utils/      (探索スキップ)

█████ score: 0.9+  ▓▓▓▓ 0.6-0.9  ░░░░ 0.3-0.6  (無色) 0.3未満
```

※ `hooks/`配下の`useButton.ts`は`components/`カラムには現れない（左ペインの結果一覧には別途表示される）。

### ヒートマップ色設計

```css
/* スコアを輝度にマッピング */
background: hsl(220, 80%, calc(20% + score * 60%));
/* score=0.97 → hsl(220,80%,78%) 明るいブルー  */
/* score=0.30 → hsl(220,80%,38%) 暗いブルー    */
/* score=0.00 → 無色（背景デフォルト）          */
```

### 実装方式（CSS変数注入）

各カラム項目に `.heat-overlay`（`position: absolute; inset: 0`）を重ね、スコアに応じて
JS側でインラインスタイルとして以下のCSS変数を注入する。固定クラス（`.heat.h9`等の8段階）は廃止し、
連続値で輝度を表現する。

```css
.heat-overlay {
  background: var(--heat-bg, transparent);
  opacity: calc(var(--heat-opacity, 0) * 0.18);
}
```
- `--heat-bg`: `hsl(220, 80%, calc(20% + score * 60%))`
- `--heat-opacity`: スコア値（0.0〜1.0）をそのまま渡し、CSS側で `* 0.18` にスケール
- `--heat-text`: 背景輝度に応じた可読文字色（`.col-score` バッジに適用）
- `found` 結果のみスコアバッジ（`.col-score`）を表示し、`opened`/`skipped` は不透明度のみで表現する

### UIの動作フロー

1. A*探索開始と同時にTauriイベントストリームでカラムが左から右へ逐次展開
2. 探索スキップされたフォルダは薄いハイライトで「見えるが掘られない」状態を表示
3. 最終結果到達時にパスラインが光るアニメーション（Framer Motion）
4. 日本語ファイル名はよみがなフリガナをツールチップ表示

---

## 3. その他デザイン方針

- キーボードファースト（マウス不要で完結）
- ダークモード標準対応
- 60fps アニメーション（Framer Motion）

### ウィンドウ装飾

OS標準タイトルバーは使用せず `decorations: false`（`tauri.conf.json`）でフレームレス化し、
独自タイトルバー（`WindowControls` コンポーネント）に最小化・最大化・閉じるボタンを実装する。
`@tauri-apps/api/window` の `getCurrentWindow()` 経由で `minimize()` / `toggleMaximize()` / `close()` を呼び出し、
タイトルバーのダブルクリックでも最大化トグルする。

---

## 4. ストリーミング／レンダリング最適化（60fps維持の肝）

- **A*ログのコアレス送信**：探索ログを1イベント/ノードで `emit` するとIPCが氾濫しメインスレッドを詰まらせ60fpsを阻害する。Tauri v2 の **`Channel` API** を使い、Rust側で **~16ms単位にバッチ束ね（coalesce）** して送る。
- **結果リストの仮想化**：左ペインAI候補や検索結果は仮想スクロール（可視行のみDOM化）で大量件数でも軽量に。
- **GPU合成寄せ**：大量のヒートマップ要素のアニメーションは Framer Motion のレイアウトアニメーションより、**CSS `transform`/`opacity` or Web Animations API** でコンポジタスレッドに載せる（レイアウト/ペイントを発生させない）。Framer Motion はパスライン等の少数要素の演出に限定する。
- 効果検証指標：A*探索中のフレームレート(fps)。

---

*作成日: 2026-06-08 / 更新日: 2026-06-11*
