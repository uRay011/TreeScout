# TreeScout — 開発フェーズ計画・リスク・参考リンク

> 関連ドキュメント: [architecture.md](architecture.md)（全体構成・検索フロー） / [backend.md](backend.md)（Rustコア実装詳細） / [frontend.md](frontend.md)（UI設計）

---

## 1. 開発フェーズ計画

### Phase 1 — MVP (4週間) ✅ 完了
- [x] Tauri v2 プロジェクト初期化
- [x] Everything DLL Rust FFI バインディング
- [x] 基本検索UI（cmdk ベース）
- [x] キーワード検索の動作確認（日英両対応）

### Phase 2 — A*エンジン＋セマンティック検索 (5週間)
- [x] Lindera 形態素解析統合（`crates/nlp` — feature フラグで ipadic opt-in）
- [x] ルールベースパーサー実装（日付・サイズ・拡張子・パス → Everything構文変換）
- [x] 静的埋め込み（model2vec系）による埋め込み生成（フォールバックで e5-small INT8）
- [x] **A*探索エンジン実装（Rustクレートとして独立実装・単体テスト）**
- [x] **2フェーズ検索フロー統合（Everything → 仮想ツリー → A*）** ※ nlp の UI 接続もここで実施
- [x] **λ・μパラメータのチューニング評価セット作成**
- [ ] LLMオプトイン統合（任意：Qwen2.5-0.5B で曖昧クエリ補助）※保留中、Phase 5以降に再検討

### Phase 3 — モデル改修 (3週間) ※任意・保留中
> LLM活用は一旦保留。Phase 5完了後、必要性を再評価してから着手判断する。
- [ ] 学習データ収集（クエリ変換ペア500〜1000件）
- [ ] Unsloth + llm-jp-3-1.8b LoRAファインチューニング
- [ ] GGUF量子化・精度評価（改修前後の比較）
- [ ] 改修モデルの組み込みテスト

詳細は [backend.md §3.3](backend.md#3-モデル改修の検討phase-3任意保留中) を参照。

### Phase 4 — 高度化＋探索UI (4週間)
- [x] SQLite 埋め込みBLOB（int8）永続化＋mmap常駐セットアップ（コアはANN拡張なし。sqlite-vecはグローバル意味検索追加時のみ）
- [x] フォルダembeddingの事前インデックス（バックグラウンド常駐） ※ `crates/index/src/folder_indexer.rs` + `index_folders_command`（`src-tauri/src/folder_index.rs`）でTauriコマンド統合済み（[backend.md §4.3](backend.md#43-フォルダ事前インデックスバックグラウンド常駐)）
- [x] **探索型カラムUI実装（Framer Motionスライドイン）**
- [x] **ヒートマップ色設計・スコア→輝度マッピング実装**
- [x] **A*探索ログのTauriイベントストリーミング → UIリアルタイム更新**
- [x] **AIガイドパスライン（最高スコアルートの光るアニメーション）**
- [x] ファイルプレビュー機能（バックエンド: `crates/preview` + `get_preview` コマンド。テキスト/Markdown/画像対応、PDF・Office等は対象外。フロント未統合。詳細: [backend.md §6](backend.md#6-ファイルプレビュー機能)）

### Phase 5 — Polish (2週間)
- [ ] キーボードショートカット完備
- [ ] パフォーマンスプロファイリング（A*探索 <150ms の確認）
- [ ] インストーラー作成 (Tauri updater)

---

## 2. 技術リスクと対策

| リスク | 内容 | 対策 |
|--------|------|------|
| Everything依存 | Everything未起動時に検索不可 | フォールバック: Windows Search API |
| LLM遅延 | 低スペックPCで推論が遅い | Qwen2.5-0.5Bへのフォールバック |
| VRAM不足 | GPU推論不可のPC | CPU推論モード（llama.cpp対応済み）|
| FFI安全性 | Rust-DLL間のメモリ安全性 | unsafe ブロックを最小化、ラッパー層を厚くする |
| 日本語クエリ精度 | LLMが検索構文に誤変換 | LoRA改修 or システムプロンプトのFew-shot強化 |
| 学習データ不足 | LoRA改修に十分なペアが集まらない | GPT-4oで合成データ生成（オフライン後に削除）|
| モデルライセンス | 改修・再配布の制約 | llm-jp-3（Apache 2.0）を使用、Phi/Llamaは要確認 |
| **A*ヒューリスティック精度** | **フォルダ名embeddingの精度が低いと探索が最適でない枝に集中** | **評価用テストセットでPrecision@K計測、λ・μを自動調整** |
| **A*探索の見逃し** | **λ大設定時に深い有望ファイルがスキップされる** | **K件未達時は自動的にλを下げて再探索するフォールバック実装** |
| **日英クロス検索精度** | **日本語クエリで英語ファイル名（またはその逆）にヒットしない** | **multilingual-e5-smallのembedding空間で日英を統一表現、評価セット整備** |
| **静的埋め込みの精度不足** | **model2vec系がe5に比べ短文以外で劣化する可能性** | **Precision@Kで段階評価し、不足時のみe5-small INT8へフォールバック（feature切替）** |
| **辞書バンドル肥大** | **Lindera ipadic辞書(~50MB+)で<30MB目標を超過** | **辞書の外部ファイル化 / unidic-mini / 圧縮配置をインストーラサイズで判断** |
| **IPC氾濫で60fps低下** | **A*ログの細粒度emitがメインスレッドを詰まらせる** | **Tauri Channel APIで~16msコアレス送信、結果リスト仮想化** |

---

## 3. 比較：技術スタック候補評価

| 候補 | パフォーマンス | 開発容易性 | バンドルサイズ | 総合 |
|------|--------------|-----------|--------------|------|
| **Tauri + Rust** | ★★★★★ | ★★★☆☆ | ★★★★★ | **推奨** |
| Electron + Node.js | ★★★☆☆ | ★★★★★ | ★★☆☆☆ | サブ候補 |
| WinUI 3 + C# | ★★★★☆ | ★★★★☆ | ★★★★☆ | Windows専用 |
| Flutter Desktop | ★★★★☆ | ★★★☆☆ | ★★★☆☆ | モバイル流用可 |

---

## 4. 参考リソース

**検索エンジン**
- [Everything SDK](https://www.voidtools.com/support/everything/sdk/) — DLL/IPC API仕様

**フレームワーク**
- [Tauri v2 Docs](https://v2.tauri.app/) — フレームワーク公式ドキュメント

**LLM推論**
- [llama.cpp](https://github.com/ggerganov/llama.cpp) — 量子化LLM推論エンジン
- [fastembed-rs](https://github.com/Anush008/fastembed-rs) — Rust埋め込みライブラリ

**日本語特化モデル**
- [llm-jp-3-1.8b](https://huggingface.co/llm-jp/llm-jp-3-1.8b) — NII製、Apache 2.0、改修推奨ベース
- [Qwen2.5-3B-Instruct](https://huggingface.co/Qwen/Qwen2.5-3B-Instruct) — 高品質日英バイリンガル
- [multilingual-e5-small](https://huggingface.co/intfloat/multilingual-e5-small) — 日英埋め込みモデル（A*ヒューリスティック用）

**モデル改修**
- [Unsloth](https://github.com/unslothai/unsloth) — 省メモリ・高速LoRAファインチューニング
- [llama.cpp GGUF変換ガイド](https://github.com/ggerganov/llama.cpp/blob/master/docs/development/GGUF.md) — モデルのGGUF量子化手順

**日本語処理**
- [Lindera](https://github.com/lindera/lindera) — Rust製形態素解析器（MeCab互換、IPAdic）

**ベクトルDB / UI**
- [sqlite-vec](https://github.com/asg017/sqlite-vec) — SQLiteベクトル拡張（sqlite-vss後継・純C/軽量。グローバル意味検索追加時のみ採用）
- [model2vec](https://github.com/MinishLab/model2vec) — 静的埋め込み（transformer蒸留・NN非通過・超軽量）
- [cmdk](https://cmdk.paco.me/) — コマンドパレットUIコンポーネント
- [shadcn/ui](https://ui.shadcn.com/) — Tailwindベースコンポーネント集
- [Framer Motion](https://www.framer.com/motion/) — Reactアニメーションライブラリ（カラムUI・ヒートマップ用）

---

*作成日: 2026-06-08 / 更新日: 2026-06-11*
