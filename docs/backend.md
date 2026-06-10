# TreeScout — バックエンド（Rustコア）設計

> 関連ドキュメント: [architecture.md](architecture.md)（全体構成・検索フロー） / [frontend.md](frontend.md)（UI設計） / [status.md](status.md)（フェーズ計画・リスク・参考リンク）

---

## 1. フレームワーク：Tauri v2

| 項目 | 内容 |
|------|------|
| 選定理由 | Electronより90%軽量、RustバックエンドでA*エンジン・DLL呼び出しを高性能処理 |
| バンドル | ~5MB（Electron は ~120MB） |
| Windows対応 | WebView2 (Chromium) ネイティブ統合 |
| セキュリティ | サンドボックス化されたIPC通信 |
| ストリーミング | A*探索の中間結果をeventで逐次UIへ送信 |

**代替候補：**
- `WinUI 3 + C#` — Windowsネイティブだが、LLMバインディングが複雑
- `Electron + Node.js` — 成熟エコシステムだが重い
- `Flutter Desktop` — クロスプラットフォームだが、FFI開発が煩雑

---

## 2. ファイル検索：Everything SDK

```
Everything.dll
├── Everything_SetSearch(query)       // クエリセット
├── Everything_Query(wait)            // 同期検索実行
├── Everything_GetNumResults()        // 結果件数
├── Everything_GetResultFileName(i)   // ファイル名取得
└── Everything_GetResultFullPathName  // フルパス取得
```

**役割：A*探索の前段フィルタ**
- 全ファイルにA*をかけると O(n) で遅すぎる
- Everythingで候補を 全件 → 1000件 以下に絞り、A*の探索空間を圧縮する
- 絞り込んだパスセットを仮想ツリーに再構築 → A*エンジンへ渡す

**Rust FFI バインディング例：**
```rust
#[link(name = "Everything64")]
extern "C" {
    fn Everything_SetSearchW(lpSearchString: LPCWSTR);
    fn Everything_QueryW(bWait: BOOL) -> BOOL;
    fn Everything_GetNumResults() -> DWORD;
}
```

**高度なクエリ構文（LLMが生成）：**
```
ext:pdf dm:2024/01 size:>1mb "project report"
ext:rs,toml path:src/ modified:lastweek
```

未起動時のフォールバックは Windows Search API。

---

## 3. ローカルLLM・埋め込みモデル

### 3.1 LLM：llama.cpp + Rust バインディング（オプション）

**コアは embedding＋Rustルールベースで完全動作する。LLMは不要。**
曖昧なクエリの補助や説明生成が必要になった場合のみオプトインする。

| 用途 | 役割 |
|------|------|
| 曖昧クエリ補助 | ルールベースで解析できない自然言語クエリの補完（最終手段） |
| 説明生成 | 「なぜこのファイルがヒットしたか」の説明（オプション） |

**推奨モデル（日本語・英語特化）：**

| モデル | サイズ | VRAM | 日本語品質 | 用途 |
|--------|--------|------|-----------|------|
| **静的埋め込み（model2vec/potion系）** | **~8〜30MB** | **不要** | ★★★★☆ | **埋め込み第一候補**：A*ヒューリスティック・ファイル名類似度 |
| multilingual-e5-small (INT8) | ~30MB | 不要 | ★★★★★ | 埋め込みフォールバック（静的が精度不足時） |
| multilingual-e5-small (FP32) | ~118MB | 不要 | ★★★★★ | ベースライン（精度上限の確認用） |
| Qwen2.5-0.5B-Instruct (INT4) | ~250MB | 1GB | ★★★★☆ | 曖昧クエリ補助（LLMオプトイン時） |
| **llm-jp-3-1.8b (Q4_K_M)** | ~1.1GB | 2GB | ★★★★★ | 日本語特化・LoRA改修ベース（Apache 2.0） |

> **all-MiniLM-L6-v2 から変更**：英語特化モデルのため日本語ファイル名の類似度が低精度になる。
> `multilingual-e5-small`（Microsoft製）は日英両対応で同等サイズ。A*のヒューリスティック評価にも使用。

### 3.2 埋め込み方式：静的埋め込みを第一候補に（速度・軽量化の最大レバー）

A*はフォルダ名・ファイル名という**短文**を大量に評価する。ここがホットパスであり、埋め込み推論方式の選択がアプリ全体の速度・バンドルサイズを最も大きく左右する。

| 方式 | 1件あたり推論 | ネイティブ依存 | サイズ | 日英短文精度 |
|------|--------------|--------------|--------|------------|
| **静的埋め込み（model2vec/potion蒸留）** | **サブms（NN非通過・ルックアップ＋mean pool）** | **なし（純Rust化可）** | **~8〜30MB** | ★★★★☆（短文で劣化小） |
| e5-small (INT8, fastembed/ONNX) | 数ms〜十数ms | ONNX Runtime `ort` (~10〜15MB) | ~30MB＋ランタイム | ★★★★★ |

- **静的埋め込み**は transformer をトークン埋め込みのルックアップ＋平均プーリングへ蒸留したもの。推論時にニューラルネットを通さないため**サブms/件**で、`fastembed` が引き込む ONNX Runtime ネイティブ依存（~10〜15MB）ごと削除でき**純Rust化**できる。
- フォルダ名/ファイル名のような短文用途は静的埋め込みでの劣化が小さく、A*の用途と相性が良い。
- **採否は計画済みの Precision@K 評価セットで判断する**。評価順序を以下に固定する：

```
① 静的埋め込み（model2vec/potion系）  ← まずここ。最軽量・最速
       │ Precision@K が閾値未満なら ↓
② multilingual-e5-small (INT8 / ONNX)  ← フォールバック
       │ それでも不足なら ↓
③ 日英特化蒸留モデル（工数大、Step 2以降）
```

> 静的埋め込みの日英精度は本検討時点の知見ベース。実装着手時に最新の model2vec 多言語モデルの精度・サイズを再確認すること。

**Rustクレート選定：**
```toml
[dependencies]
llama-cpp-2 = "0.1"          # llama.cpp Rustバインディング（LLMはオプション）
model2vec-rs = "*"           # 静的埋め込み（第一候補・純Rust・ONNX不要）
# fastembed = "3"            # 埋め込みフォールバック（e5-small INT8）。静的が精度不足の時のみ有効化
tokenizers = "0.20"           # HuggingFace tokenizer（改修時に必要）
```

> `fastembed` は ONNX Runtime ネイティブ依存を引き込むため、**デフォルトでは無効**にして静的埋め込みで動かす。Precision@K がフォールバックを要求したときだけ feature で有効化する設計にする。

### 3.3 モデル改修の検討（Phase 3・任意/保留中）

**改修難易度と効果：**

| 手法 | 難易度 | 効果 | 必要データ | 説明 |
|------|--------|------|-----------|------|
| **プロンプトエンジニアリング** | ★☆☆☆☆ | 中 | 不要 | システムプロンプトで日本語検索クエリ生成を専用チューニング。**まずここから** |
| **LoRA ファインチューニング** | ★★★☆☆ | 大 | ~1000件 | 検索クエリ変換タスクに特化した軽量アダプター追加 |
| **GGUF量子化（自前ビルド）** | ★★☆☆☆ | 中 | 不要 | llm-jp等のモデルをGGUF化して高速化・軽量化 |
| **埋め込みモデル追加学習** | ★★★☆☆ | 大 | ~5000件 | ファイル名/パスの日本語特化埋め込みに調整 |
| **フルファインチューニング** | ★★★★★ | 最大 | ~10万件 | GPU必須、難易度高、過剰投資になる可能性大 |

**最小工数で効果大：LoRAによるクエリ変換特化（推奨改修案）**

```
学習データ例（JSONL形式、~500〜1000件で十分）:
{"input": "先週保存したExcelファイル", "output": "ext:xlsx,xls modified:lastweek"}
{"input": "srcフォルダのTypeScriptファイル",  "output": "ext:ts,tsx path:src/"}
{"input": "大きいPDFファイル",               "output": "ext:pdf size:>10mb"}
{"input": "readme files",                   "output": "file:readme*"}
```

**LoRA改修ツールチェーン：**
```
Unsloth（高速・省メモリLoRA）
  └── llm-jp-3-1.8b ベースモデル（Apache 2.0ライセンス、改変可）
        └── ファインチューニング → GGUF変換（llama.cpp）→ 組み込み
```

---

## 4. データストア・ベクトル検索

### 4.1 コアフローはANN不要（必要時のみ sqlite-vec）

> **方針転換：`sqlite-vss` は採用しない。**
> `sqlite-vss` はメンテナンス終了済みで内部に Faiss を抱え、重く Windows ビルドが困難。後継は同作者の **`sqlite-vec`**（純C・追加依存なし・軽量・Windowsビルド容易）。
>
> さらに本質的な点として、**Everything で候補を ~1000件に絞った後の意味検索に ANN インデックスは不要**。1000件 × 384次元の cosine 類似度を **SIMD で総当たり**すれば数十〜数百μsで完了し、ANNの構築/クエリより速く実装も単純。埋め込みは BLOB として保持し、Rust側でブルートフォース計算する。
>
> | フェーズ | ベクトル検索手段 |
> |---------|----------------|
> | **コアフロー（Everything→A*）** | **ANN拡張なし。Rust側 SIMD ブルートフォース cosine** |
> | グローバル意味検索（Everything非経由・全件対象）を将来追加する場合のみ | `sqlite-vec` を採用 |
>
> 効果検証指標：vss削除によるバンドルサイズ差分(MB) と、1000件 cosine の実測時間(μs)。

日本語ファイル名は形態素解析（後述）と組み合わせてベクトル化精度を高める。

```
ファイルメタデータ DB (SQLite)
├── files テーブル
│   ├── path TEXT
│   ├── name TEXT
│   ├── name_reading TEXT       ← 日本語よみがな（形態素解析で付与）
│   ├── ext TEXT
│   ├── size INTEGER
│   ├── modified DATETIME
│   ├── embedding BLOB (384次元, int8量子化 ≈ 384B/件)  ← float32の1/4。SIMD int8内積で高速
│   └── emb_mtime INTEGER  ← 埋め込みキャッシュの鮮度判定キー（(path,mtime)）
├── folders テーブル
│   ├── path TEXT
│   └── folder_embedding BLOB  ← A*ヒューリスティック用（バックグラウンド事前計算）
└── （ANN仮想テーブルは持たない）
    Everything絞り込み後の~1000件はRust側SIMDブルートフォースでcosine。
    将来グローバル意味検索を足す時のみ sqlite-vec を追加。
```

> **埋め込みの保存形式**：float32(1536B)→**int8量子化(384B)**でDBを1/4にし、スキャンも SIMD int8内積で高速化する。A*が多用するフォルダ埋め込みは、SQLite行単位BLOB取得よりキャッシュ効率の高い**メモリ上の連続行列(mmap)**として保持する。
> **埋め込みキャッシュ**：ファイル埋め込みは `(path, mtime)` をキーに永続化し、検索ごとの再計算を回避。更新検知は Everything が既に利用する **Windows USN ジャーナル**で差分のみ再計算する（全件再スキャンを避ける）。

### 4.2 日本語トークナイズ：Lindera (Rust製形態素解析)

```toml
lindera = { version = "0.33", features = ["ipadic"] }  # IPAdic辞書同梱
```
- 複合語を分割してベクトル化 → embedding精度向上（例:「議事録作成手順書」→「議事録/作成/手順書」）
- 日英クロス検索：日本語クエリで英語ファイル名にもヒット（embedding空間で接続）
- ※ Everythingクエリには分割後トークンを使わない（部分文字列マッチのためノイズになる）
- pure Rustで動作、MeCabのような外部インストール不要

> ⚠ **バンドルサイズ注意**：`ipadic` feature は辞書(~50MB+)をバイナリに埋め込み、**<30MB目標を単独で超過する**。対策候補（実装時にインストーラサイズで判断）：
> 1. 辞書を**外部ファイル化**（バイナリ埋め込みをやめ、インストーラ同梱の別ファイルとして配置）
> 2. `unidic-mini` 等の**小型辞書**に切替
> 3. 辞書の**圧縮配置**＋起動時展開
>
> 形態素解析はクエリ側の短い入力にしか使わないため、辞書は最小構成で十分な可能性が高い。

**代替：** DuckDB + `vss` 拡張（分析クエリが得意）

### 4.3 フォルダ事前インデックス（バックグラウンド常駐）

`index_folders_command(root)` を呼ぶと以下を実行する：

1. `crates/index::index_folders` — `root` 配下を `WalkDir` で走査し、ディレクトリの `mtime` をキーに差分判定。変更分のみフォルダ名をバッチ embedding して `IndexStore`（SQLite）に upsert
2. `crates/index::rebuild_folder_matrix` — 上記結果から `FolderEmbeddingMatrix` を mmap ファイル（`folders.bin`）として再構築

**現状の暫定実装（要置き換え）：**
- 埋め込みモデル（`embedding::StaticEmbedder` / model2vec）は未バンドルのため、`src-tauri/src/folder_index.rs` の `DummyEmbedder`（文字数ベースの非意味的ベクトル）で配線確認のみ実施している
- モデル統合時は `DummyEmbedder` を `StaticEmbedder` に差し替える（`FOLDER_EMBEDDING_DIM` も実モデルの次元数に合わせて変更）
- フロントからの呼び出しトリガー（自動実行タイミング・進捗表示）は未実装

---

## 5. A*探索エンジン

Everything絞り込み後の仮想ツリー上でA*を走らせ、全ファイルをベクトル化せずに上位K件を効率的に発見する。

**コスト関数：**
```
f(node) = g(node) + h(node)

g(node): 探索コスト
  = depth(node) × λ          // 深いノードへのペナルティ
  + vectorized_count × μ     // ベクトル化済みファイル数コスト

h(node): ヒューリスティック（推定関連度）
  = cosine_sim(folder_embedding, query_embedding)
  ※ フォルダ名の事前embeddingで計算（軽量・高速）

λ = 0   : 貪欲探索（最高スコアの枝に集中）
λ 大    : 浅いファイル優先（広く薄く探索）
μ       : 探索深度vs精度のトレードオフ調整
```

**2段階ベクトル化（コスト削減の肝）：**
```
[事前インデックス（バックグラウンド常駐）]
  フォルダ名のみembedding → SQLite保存
  例: "components/Button/" → [0.2, 0.8, ...]  ← 軽量・ms単位

[A*探索時（オンデマンド）]
  A*が「開いた」ディレクトリのファイルのみベクトル化
  → 全ファイルの 5〜10% のベクトル化で高品質上位K件が得られる想定
```

**ホットパス最適化（速度の肝）：**
- **バッチ推論**：A*が1ノードを開いた際、配下ファイル群は**1回のバッチ呼び出し**でまとめて埋め込む。1件ずつ呼ぶと固定オーバーヘッドで数倍遅くなる。
- **永続キャッシュ＋差分更新**：埋め込みは `(path, mtime)` キーでSQLiteに永続化し、検索ごとの再計算を回避。Windows USNジャーナルで変更ファイルのみ再計算する。
- **int8内積**：スコア計算は int8量子化埋め込みの SIMD 内積で行い、float32展開を避ける。
- **フォルダ埋め込みのmmap常駐**：A*ヒューリスティックで多用するフォルダ埋め込みは連続行列としてmmap常駐させ、行単位BLOB取得のキャッシュミスを排除する。

**Rust実装：**
```rust
use std::collections::BinaryHeap;
use std::cmp::Reverse;

struct SearchNode {
    path:     PathBuf,
    f_score:  f32,          // A*スコア（= h - g、最大ヒープ用に符号反転）
    g_cost:   f32,          // 探索コスト
    h_score:  f32,          // ヒューリスティック（フォルダembedding類似度）
    is_file:  bool,
}

// 優先度付きキュー（最大ヒープ）
let mut queue: BinaryHeap<SearchNode> = BinaryHeap::new();

// 探索ループ
while let Some(node) = queue.pop() {
    if results.len() >= K { break; }
    if node.is_file {
        // ファイル到達: 精密ベクトル化してスコア確定
        let score = embed_and_score(&node.path, &query_vec);
        results.push((node.path, score));
    } else {
        // ディレクトリ展開: 子ノードをヒューリスティック評価してキューへ
        for child in expand(&node.path) {
            let h = folder_embedding_sim(&child, &query_vec);
            let g = depth(&child) as f32 * λ;
            queue.push(SearchNode { f_score: h - g, g_cost: g, h_score: h, .. });
        }
    }
}
```

**性能見積もり：**

| 処理 | 従来（全件ベクトル化） | A*適用後 |
|------|---------------------|---------|
| ベクトル化ファイル数 | 10万件 | ~500件（5%） |
| 探索時間 | 数十秒 | <150ms |
| 上位K件精度 | 100% | ~90%（上位結果は同等） |

> **Beam Search / Best-First Searchとの関係**：ファイル検索は「最短経路保証」が不要なため、
> 厳密なA*のadmissibility制約は課さない。λ・μのチューニングで精度と速度を実行時調整できる
> Best-First Searchの亜種として実装する。

---

## 6. ファイルプレビュー機能

検索パイプラインとは完全に分離した経路（アイテム選択時のみ呼び出し）。`<200ms` 目標には影響しない。

- **対応種別**
  - テキスト/コード（拡張子ベース判定）: 先頭64KBを非同期読込、UTF-8として返却（`truncated`フラグ付き）
  - Markdown: テキストと同様に取得し、レンダリングはフロント側（remark等）で行う
  - 画像（png/jpg/jpeg/gif/webp/bmp/svg/ico）: バックエンドは種別判定のみ返し、本体は `convertFileSrc` でフロントが直接参照（IPCシリアライズ・サムネイル生成コストを回避）
  - 上記以外: `Unsupported`（PDF/Office/動画はPhase 5以降で再検討）
- **未実装（フロント側）**
  - カラムUI選択イベントとの接続
  - 選択変更時の前回プレビューキャンセル・デバウンス（100〜150ms目安）

実装: `crates/preview` + `get_preview` コマンド。

---

## 7. パフォーマンス・バンドル最適化チェックリスト（実装時）

**Rustリリースプロファイル（`Cargo.toml`）— 数百KB〜MB単位の削減：**
```toml
[profile.release]
lto = "fat"          # リンク時最適化
opt-level = "z"      # サイズ優先（速度優先なら "s" や 3 を計測比較）
panic = "abort"      # アンワインドテーブル削除
codegen-units = 1    # 最適化機会を最大化
strip = true         # シンボル除去
```

**Everything FFI のラウンドトリップ削減：**
- `Everything_SetRequestFlags` で必要列（フルパスのみ等）に限定し、1000件分の余分な列取得・UTF-16変換コストを削る。
- 結果取得は件数分の個別FFI呼び出しを避け、可能な限りまとめて読む。

**埋め込み／スコアリング：**
- int8量子化埋め込み + SIMD内積（§4/§5）。
- バッチ推論・(path,mtime)永続キャッシュ・USN差分更新（§5）。

---

*作成日: 2026-06-08 / 更新日: 2026-06-11*
