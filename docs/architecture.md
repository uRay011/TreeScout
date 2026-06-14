# TreeScout — アーキテクチャ・検索フロー設計

> Everything SDK × A*探索エンジン × ローカルLLM（日本語・英語特化）× 探索型ヒートマップUI による超高速セマンティックファイル探索
>
> 関連ドキュメント: [backend.md](backend.md)（Rustコア実装詳細） / [frontend.md](frontend.md)（UI設計） / [status.md](status.md)（フェーズ計画・リスク・参考リンク）

---

## 1. コンセプト

```
[自然言語クエリ] → [LLM意図解析] → [Everything DLL高速絞り込み]
                                           ↓
                              [検索結果（ベースライン・全件）]
                                           ↓
                              [A*ツリー探索エンジン]
                         (f = g + h で有望パスを優先展開)
                                           ↓
              [スコアオーバーレイ＋追加サジェスト（pathでdedup統合）]
                                           ↓
                         [AIガイドパスライン＋ヒートマップUI]
```

| 要素 | 目標 |
|------|------|
| 検索速度 | Everything絞り込み <50ms ＋ A*探索 <150ms → 総合 <200ms |
| AI応答 | ローカル推論 <500ms（量子化モデル利用） |
| UI | 探索型カラムUI・ヒートマップ / 60fps スムーズ操作 |
| プライバシー | 完全オフライン動作（データ外部送信なし） |
| バンドルサイズ | <30MB（LLM除く） |

---

## 2. システムアーキテクチャ

```
┌─────────────────────────────────────────────────────────────┐
│                    Frontend (UI Layer)                       │
│         React + TypeScript + Tailwind CSS                    │
│    探索型カラムUI / ヒートマップ / AIガイドパスライン表示        │
│           Tauri WebView (WebKit2GTK / WebView2)              │
└──────────────────────┬──────────────────────────────────────┘
                       │ Tauri IPC (invoke / streaming events)
┌──────────────────────▼──────────────────────────────────────┐
│                  Backend (Rust Core)                         │
│  ┌───────────────┐  ┌──────────────┐  ┌────────────────┐   │
│  │ Everything FFI│  │  LLM Engine  │  │ A* Search      │   │
│  │  (DLL bridge) │  │(llama.cpp-rs)│  │ Engine         │   │

│  └───────┬───────┘  └──────┬───────┘  └───────┬────────┘   │
│          │                 │                   │            │
│  ┌───────▼─────────────────▼───────────────────▼────────┐   │
│  │                    Indexer (SQLite)                   │   │
│  │  ファイルメタ / フォルダembedding / 探索ログキャッシュ   │   │
│  └───────────────────────────────────────────────────────┘   │
└──────────┬────────────────┬───────────────────┬─────────────┘
           │                │                   │
     ┌─────▼──────┐  ┌──────▼──────┐  ┌────────▼──────┐
     │Everything  │  │ GGUF Model  │  │ SQLite        │
     │.dll / SDK  │  │(任意・LLM)  │  │ +埋め込みBLOB │
     └────────────┘  └─────────────┘  └───────────────┘
       ※意味検索はEverything絞込後にRust側SIMDブルートフォース（ANN拡張なし）
```

---

## 3. 検索フロー詳細

### 2フェーズ検索アーキテクチャ

```
Phase 1: Everything SDK（キーワード絞り込み）              数ms
  全ファイル（数百万件）→ キーワードマッチ → 候補1000件以下
  → そのままベースライン結果として表示（取りこぼしゼロを保証）

Phase 2: A*ツリー探索（スコアリング＋追加サジェスト）     <150ms
  候補1000件の仮想ツリー → A*で有望パスを優先展開
    ・候補内のファイル   → ヒートマップ用スコアを付与
    ・候補外のディレクトリ → 高スコアパスのみ追加探索しAIサジェストとして提示
  Phase1結果 ∪ AIサジェスト（pathでdedup）→ 表示

合計: <200ms
```

### 日本語クエリ例

```
ユーザー入力: "先週編集したReactのコンポーネントファイル"
          │
          ├─[言語検出] → 日本語
          │
          ▼
  [Lindera 形態素解析]
  "先週/編集/した/React/の/コンポーネント/ファイル"
  ※ embedding精度向上・クロス言語検索のためのトークン化（Everythingクエリには直接使用しない）
          │
          ▼
  [ルールベースパーサー（Rust）]
  日付・サイズ・拡張子・パス → Everything構文に変換
  残りのキーワード → セマンティック検索用クエリとして分離
  ※ LLMは不要。正規表現＋辞書で99%カバー
          │
          ▼ 構造化クエリ生成 (JSON)
  {
    "keywords": ["React", "component"],
    "ext": ["tsx", "jsx"],
    "date_filter": "modified:lastweek",
    "everything_query": "ext:tsx,jsx modified:lastweek React"
  }
          │
          ▼
  [Phase 1: Everything DLL] → 候補: ~800件 (数ms)
  パスセットを仮想ツリーに再構築 → ベースライン結果として保持（全件表示対象）
          │
          ▼
  [Phase 2: A*探索エンジン]
  query_vec = embed("Reactコンポーネント")  ← 静的埋め込み（フォールバック: e5-small INT8）
  優先キューで f=g+h 最大ノードを展開
  開いたディレクトリのファイルのみオンデマンドベクトル化
          │
          ▼
  [探索ログ（UIへストリーミング）]
  opened: src/            h=0.82
  opened: components/     h=0.91  ← 優先展開
  skipped: utils/         h=0.12  ← スキップ（ヒートマップで薄表示）
  found:   Button.tsx     f=0.97  ← Phase1候補内・スコア付与
  found:   useButton.ts   f=0.71  ← Phase1候補内・スコア付与
  suggest: docs/Button.md f=0.65  ← Phase1候補外・AIサジェスト
          │
          ▼
  [Phase1結果(全件) ∪ AIサジェスト（pathでdedup）]
  → カラムUI表示。Phase1結果はそのまま、AIサジェストはバッジ付きで提示
  (総合 <200ms)
```

### 英語クエリ例

```
ユーザー入力: "large pdf files from last month"
          │
          ├─[言語検出] → 英語（形態素解析スキップ）
          │
          ▼
  [LLM クエリパーサー]
  → "ext:pdf size:>5mb modified:lastmonth"
          │
          ▼ (以降 Phase 1 / Phase 2 同様)
```

### ルートフォルダ指定によるスコープ絞り込み

ユーザーがフォルダピッカー（`@tauri-apps/plugin-dialog`）または手入力で探索ルートを指定した場合、
`semantic_search` の `root_path` 引数として渡され、Everythingクエリに `path:"<root>"` として合成される。

```
root_path あり + クエリあり → path:"<root>" <everything_query>
root_path あり + クエリなし → path:"<root>"                     ← ルート配下の一覧表示
root_path なし              → <everything_query>（従来通り）
```

ルート確定時はクエリが空でも即座に検索を実行し、配下の一覧をカラムUIに表示する（フォルダブラウズの起点）。

### 言語検出（軽量・ライブラリ不要）

```rust
// 文字コード範囲で判定（外部クレート不要）
fn detect_lang(input: &str) -> Lang {
    let ja_chars = input.chars()
        .filter(|c| matches!(c, '\u{3000}'..='\u{9FFF}' | '\u{FF00}'..='\u{FFEF}'))
        .count();
    if ja_chars > 0 { Lang::Japanese } else { Lang::English }
}
```

---

*作成日: 2026-06-08 / 更新日: 2026-06-12*
