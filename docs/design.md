# TreeScout — 次世代ファイル探索システム 技術スタック検討書

> Everything SDK × A*探索エンジン × ローカルLLM（日本語・英語特化）× 探索型ヒートマップUI による超高速セマンティックファイル探索

---

## 1. コンセプト

```
[自然言語クエリ] → [LLM意図解析] → [Everything DLL高速絞り込み]
                                           ↓
                              [A*ツリー探索エンジン]
                         (f = g + h で有望パスを優先展開)
                                           ↓
                         [セマンティックランキング（上位K件）]
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

## 3. 技術スタック詳細

### 3.1 フレームワーク：Tauri v2

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

### 3.2 ファイル検索：Everything SDK

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

### 3.3 ローカルLLM：llama.cpp + Rust バインディング（オプション）

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

#### 埋め込み方式：静的埋め込みを第一候補に（速度・軽量化の最大レバー）

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

#### モデル改修の検討

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

### 3.4 ベクトル検索：コアフローはANN不要（必要時のみ sqlite-vec）

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

**日本語トークナイズ：Lindera (Rust製形態素解析)**
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

### 3.5 A*探索エンジン

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

### 3.6 フロントエンド UI スタック

```
React 19 + TypeScript 5
├── Vite 6 (ビルドツール)
├── Tailwind CSS v4 (ユーティリティCSS)
├── Framer Motion (カラムスライドイン・ヒートマップアニメーション)
├── cmdk (コマンドパレットUI)
├── shadcn/ui (コンポーネント)
└── @tauri-apps/api (IPC通信 / A*探索ストリーミング受信)
```

**デザイン方針：AIガイドパスライン＋ヒートマップ**

A*探索が開いたパスをリアルタイムで可視化する探索型カラムUIを採用する。
AIが最高スコアのルートを「光るパス」として提示し、ユーザーは確認・逸脱が自由にできる。

```
クエリ: "Reactのボタンコンポーネント"

  src/          components/       Button/
  ████████  →   ████████████  →   █████████████   Button.tsx    ★0.97
  ▓▓▓▓▓▓   →   ▓▓▓▓▓ hooks/  →   ▓▓▓▓▓▓▓▓▓▓▓▓   useButton.ts  ★0.71
                ░░░░░ utils/       ░░░░░░░░░░░░░   (探索スキップ)

█████ score: 0.9+  ▓▓▓▓ 0.6-0.9  ░░░░ 0.3-0.6  (無色) 0.3未満
```

**ヒートマップ色設計：**
```css
/* スコアを輝度にマッピング */
background: hsl(220, 80%, calc(20% + score * 60%));
/* score=0.97 → hsl(220,80%,78%) 明るいブルー  */
/* score=0.30 → hsl(220,80%,38%) 暗いブルー    */
/* score=0.00 → 無色（背景デフォルト）          */
```

**UIの動作フロー：**
1. A*探索開始と同時にTauriイベントストリームでカラムが左から右へ逐次展開
2. 探索スキップされたフォルダは薄いハイライトで「見えるが掘られない」状態を表示
3. 最終結果到達時にパスラインが光るアニメーション（Framer Motion）
4. 日本語ファイル名はよみがなフリガナをツールチップ表示

**その他デザイン方針：**
- キーボードファースト（マウス不要で完結）
- ダークモード標準対応
- 60fps アニメーション（Framer Motion）

**ストリーミング／レンダリング最適化（60fps維持の肝）：**
- **A*ログのコアレス送信**：探索ログを1イベント/ノードで `emit` するとIPCが氾濫しメインスレッドを詰まらせ60fpsを阻害する。Tauri v2 の **`Channel` API** を使い、Rust側で **~16ms単位にバッチ束ね（coalesce）** して送る。
- **結果リストの仮想化**：左ペインAI候補や検索結果は仮想スクロール（可視行のみDOM化）で大量件数でも軽量に。
- **GPU合成寄せ**：大量のヒートマップ要素のアニメーションは Framer Motion のレイアウトアニメーションより、**CSS `transform`/`opacity` or Web Animations API** でコンポジタスレッドに載せる（レイアウト/ペイントを発生させない）。Framer Motion はパスライン等の少数要素の演出に限定する。
- 効果検証指標：A*探索中のフレームレート(fps)。

---

## 4. 検索フロー詳細

### 2フェーズ検索アーキテクチャ

```
Phase 1: Everything SDK（キーワード絞り込み）              数ms
  全ファイル（数百万件）→ キーワードマッチ → 候補1000件以下

Phase 2: A*ツリー探索（セマンティック絞り込み）          <150ms
  候補1000件の仮想ツリー → A*で有望パスを優先展開 → 上位20件

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
  パスセットを仮想ツリーに再構築
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
  found:   Button.tsx     f=0.97  ← 結果確定
  found:   useButton.ts   f=0.71
          │
          ▼
  [上位20件をカラムUI表示] (総合 <200ms)
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

## 5. 開発フェーズ計画

### Phase 1 — MVP (4週間) ✅ 完了
- [x] Tauri v2 プロジェクト初期化
- [x] Everything DLL Rust FFI バインディング
- [x] 基本検索UI（cmdk ベース）
- [x] キーワード検索の動作確認（日英両対応）

### Phase 2 — A*エンジン＋セマンティック検索 (5週間)
- [ ] Lindera 形態素解析統合
- [ ] ルールベースパーサー実装（日付・サイズ・拡張子・パス → Everything構文変換）
- [ ] 静的埋め込み（model2vec系）による埋め込み生成（フォールバックで e5-small INT8）
- [ ] **A*探索エンジン実装（Rustクレートとして独立実装・単体テスト）**
- [ ] **2フェーズ検索フロー統合（Everything → 仮想ツリー → A*）**
- [ ] **λ・μパラメータのチューニング評価セット作成**
- [ ] LLMオプトイン統合（任意：Qwen2.5-0.5B で曖昧クエリ補助）

### Phase 3 — モデル改修 (3週間) ※任意
- [ ] 学習データ収集（クエリ変換ペア500〜1000件）
- [ ] Unsloth + llm-jp-3-1.8b LoRAファインチューニング
- [ ] GGUF量子化・精度評価（改修前後の比較）
- [ ] 改修モデルの組み込みテスト

### Phase 4 — 高度化＋探索UI (4週間)
- [ ] SQLite 埋め込みBLOB（int8）永続化＋mmap常駐セットアップ（コアはANN拡張なし。sqlite-vecはグローバル意味検索追加時のみ）
- [ ] フォルダembeddingの事前インデックス（バックグラウンド常駐）
- [ ] **探索型カラムUI実装（Framer Motionスライドイン）**
- [ ] **ヒートマップ色設計・スコア→輝度マッピング実装**
- [ ] **A*探索ログのTauriイベントストリーミング → UIリアルタイム更新**
- [ ] **AIガイドパスライン（最高スコアルートの光るアニメーション）**
- [ ] ファイルプレビュー機能

### Phase 5 — Polish (2週間)
- [ ] キーボードショートカット完備
- [ ] パフォーマンスプロファイリング（A*探索 <150ms の確認）
- [ ] インストーラー作成 (Tauri updater)

---

## 6. 技術リスクと対策

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

## 6.5 パフォーマンス・バンドル最適化チェックリスト（実装時）

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
- int8量子化埋め込み + SIMD内積（§3.4/§3.5）。
- バッチ推論・(path,mtime)永続キャッシュ・USN差分更新（§3.5）。

**フロントエンド：**
- 結果リスト仮想化、ヒートマップはCSS transform/WAAPIでGPU合成（§3.6）。

---

## 7. 比較：技術スタック候補評価

| 候補 | パフォーマンス | 開発容易性 | バンドルサイズ | 総合 |
|------|--------------|-----------|--------------|------|
| **Tauri + Rust** | ★★★★★ | ★★★☆☆ | ★★★★★ | **推奨** |
| Electron + Node.js | ★★★☆☆ | ★★★★★ | ★★☆☆☆ | サブ候補 |
| WinUI 3 + C# | ★★★★☆ | ★★★★☆ | ★★★★☆ | Windows専用 |
| Flutter Desktop | ★★★★☆ | ★★★☆☆ | ★★★☆☆ | モバイル流用可 |

---

## 8. 参考リソース

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

*作成日: 2026-06-08 / 更新日: 2026-06-09*
