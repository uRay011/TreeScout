# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## プロジェクト概要

TreeScout は **Tauri v2 (Rust + React)** で構築する超高速セマンティックファイル探索デスクトップアプリ（Windows）。完全オフライン動作・プライバシー重視。

目標パフォーマンス: Everything絞り込み <50ms + A*探索 <150ms = 合計 <200ms

## 開発環境

- **ビルド・動作確認:** Windows ネイティブで実施
- **コード編集・git操作:** WSL2
- **WSL側では `cargo build` / `tauri dev` を実行しない**（Tauri v2 は Windows GUI ターゲットのため）

## 技術スタック

| レイヤー | 技術 |
|---------|------|
| フレームワーク | Tauri v2（バンドル ~5MB） |
| フロントエンド | React 19 + TypeScript 5 + Vite 6 + Tailwind CSS v4 |
| UIコンポーネント | shadcn/ui + cmdk + Framer Motion |
| バックエンド | Rust |
| ファイル検索 | Everything SDK (DLL FFI) |
| LLM推論 | llama.cpp-rs（オプション） |
| 埋め込み | 静的埋め込み（model2vec系）第一候補、不足時のみ fastembed にフォールバック |
| 形態素解析 | Lindera (`lindera = "0.33"`) |
| ベクトルDB | SQLite（int8 BLOB、Rust側 SIMDブルートフォース） |
| IPC通信 | Tauri invoke / streaming events（Channel API で ~16msコアレス） |

## 設計書

詳細な設計仕様は役割ごとに分割されている（[docs/design.md](docs/design.md) はインデックス）。

- [docs/architecture.md](docs/architecture.md) — コンセプト・全体構成・検索フロー
- [docs/backend.md](docs/backend.md) — Rustコア（Everything FFI・LLM/埋め込み・DB・A*・プレビュー）
- [docs/frontend.md](docs/frontend.md) — UI設計（カラムUI・ヒートマップ・ウィンドウ装飾）
- [docs/status.md](docs/status.md) — フェーズ計画・リスク・参考リンク

## フォルダ構成

```
TreeScout/
├── docs/
│   ├── design.md             # 設計書インデックス
│   ├── architecture.md       # コンセプト・全体構成・検索フロー
│   ├── backend.md            # Rustコア設計（Everything/LLM/DB/A*/プレビュー）
│   ├── frontend.md           # UI設計（カラムUI・ヒートマップ等）
│   └── status.md             # フェーズ計画・リスク・参考リンク
├── assets/
│   ├── mock.html             # UIモックアップ
│   └── ui_image.png
├── crates/                   # Tauri非依存のコアロジック（create-tauri-app後に作成）
│   ├── astar/                # A*探索エンジン（独立クレート・単体テスト）
│   ├── search/               # Everything FFI + Windows Search API fallback
│   ├── embedding/            # model2vec / fastembed
│   ├── nlp/                  # Lindera + ルールベースパーサー
│   ├── index/                # SQLite スキーマ・mmap管理（フォルダembedding事前インデックス）
│   ├── pipeline/             # nlp + astar + embedding + search を統合する2フェーズ検索パイプライン
│   └── preview/              # ファイルプレビュー（テキスト/Markdown/画像種別判定）
├── src-tauri/                # 薄いinvoke/Channelアダプタ（create-tauri-appが生成）
│   └── src/
│       └── folder_index.rs   # フォルダembedding事前インデックスのTauriコマンド統合
└── src/                      # React フロントエンド（create-tauri-appが生成）
    ├── components/
    │   ├── SearchBar/
    │   └── ColumnView/        # 探索型カラムUI・HeatmapItem
    ├── hooks/
    ├── lib/
    │   └── tauri.ts          # IPC ラッパー
    └── App.tsx
```

> `src-tauri/` と `src/` は Phase 1 の `create-tauri-app` scaffold が生成する。手作りしない。
> `crates/` 以下は scaffold 後にワークスペース化して追加する。

## 開発フェーズ

- **Phase 1 (MVP)**: Tauri v2初期化 → Everything DLL FFI → 基本検索UI
- **Phase 2 (AI+A*)**: Lindera → A*エンジン（Rustクレート独立実装・単体テスト） → 2フェーズ検索統合 → λ・μチューニング評価セット
- **Phase 3 (LoRA改修、任意・保留中)**: Unsloth + llm-jp-3-1.8b LoRA → GGUF変換 ※LLM活用は一旦保留、Phase 5以降に再検討
- **Phase 4 (探索UI)**: 埋め込みint8 BLOB永続化＋mmap常駐 → フォルダembedding事前インデックス → カラムUI/ヒートマップ → A*探索ログのChannelストリーミング → AIガイドパスライン → ファイルプレビュー（フロント未統合）
- **Phase 5 (Polish)**: キーボードショートカット → A*探索 <150ms プロファイリング → インストーラー

## 主要な実装上の注意点

- Everything DLL FFI は `unsafe` ブロックを最小化しラッパー層を厚くする。`Everything_SetRequestFlags` で必要列のみ要求し、1000件分の余分なUTF-16変換コストを削る
- Everything 未起動時のフォールバック: Windows Search API
- LLM 未インストール時のフォールバック: Lindera＋ルールベースパーサーで動作継続
- 埋め込みは int8 BLOB で保存し cosine は SIMD int8 内積で計算。フォルダ埋め込みは連続行列として mmap 常駐。ファイル埋め込みは `(path, mtime)` キャッシュ＋USN差分更新
- A* 探索ログは Tauri v2 `Channel` API で ~16ms 単位にコアレスして送信し、IPC氾濫による 60fps 低下を防ぐ
- リリースビルドは `lto="fat"` / `opt-level="z"` / `panic="abort"` / `codegen-units=1` / `strip=true` でバンドルを最小化する
- Lindera 辞書は ipadic 全埋め込みを避け、小型辞書/外部ファイル化/圧縮配置をインストーラサイズで判断する

## コミットルール

### タイトル
- 形式: `<type>: <内容>`（Conventional Commits 準拠）
  - type: `feat` / `fix` / `refactor` / `chore` / `docs` / `test` / `perf`
  - 例: `feat: Everything FFI ラッパー追加`, `fix: A*探索の無限ループバグ修正`
- 内容は日本語で記述する

### コードコメント
- 日本語で記述する
- WHY が非自明な場合のみ（パフォーマンス制約・回避策など）

### 運用
- コミットは人間が行う。Claude は `git commit` を実行しない