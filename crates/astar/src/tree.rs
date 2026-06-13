use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Everything の絞り込み結果から構築する仮想ツリー。
///
/// 実ファイルシステムを触らず、パス文字列だけで親子関係を管理する。
/// Everything 結果は Windows の絶対パスだが、WSL 側の単体テストでは
/// POSIX パスのまま使えるよう Path に抽象化している。
pub struct VirtualTree {
    /// 子ノード一覧 ( parent_path → Vec<child_path> )。重複なし。
    children: HashMap<PathBuf, Vec<PathBuf>>,
    /// フルパス → ファイルか否か
    is_file: HashMap<PathBuf, bool>,
    /// ルートノード群（共通祖先が複数になりうる）
    pub roots: Vec<PathBuf>,
    /// フォルダembedding行列（folders.bin）由来の追加ディレクトリ ( parent_path → Vec<dir_path> )。
    /// Phase1候補ツリーには含まれないが、`expand()` でAIサジェスト探索の対象として開放する。
    extra_dirs: HashMap<PathBuf, Vec<PathBuf>>,
}

impl VirtualTree {
    /// Everything の結果パス一覧からツリーを構築する。
    ///
    /// `file_paths` に含まれないが祖先になるディレクトリを自動補完する。
    pub fn from_paths(file_paths: &[PathBuf]) -> Self {
        let mut children: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        let mut is_file: HashMap<PathBuf, bool> = HashMap::new();
        // 既に親→子エッジを追加済みか追跡（重複防止）
        let mut edge_seen: HashSet<(PathBuf, PathBuf)> = HashSet::new();

        for fp in file_paths {
            is_file.insert(fp.clone(), true);
            let mut child = fp.clone();

            loop {
                let parent = match child.parent() {
                    // parent が空文字 ("") またはコンポーネント数が 0（"/" や "C:\" に相当）は除外
                    Some(p)
                        if !p.as_os_str().is_empty()
                            && p.components().count() >= 2 =>
                    {
                        p.to_path_buf()
                    }
                    _ => break,
                };

                is_file.entry(parent.clone()).or_insert(false);

                let edge = (parent.clone(), child.clone());
                if !edge_seen.contains(&edge) {
                    edge_seen.insert(edge);
                    children.entry(parent.clone()).or_default().push(child.clone());
                }

                // この親がすでに is_file に登録されていた = 上位は別ファイルで処理済み
                // ただし今回初登録なら上位もさかのぼる必要があるため、
                // `children` に既に存在していることではなく `edge_seen` で重複を防ぐ
                child = parent;
            }
        }

        // ルート = 自分が誰かの子でないノード（is_file に登録されているが children の値側に出てこない）
        let all_children: HashSet<PathBuf> =
            children.values().flatten().cloned().collect();
        let mut roots: Vec<PathBuf> = is_file
            .keys()
            .filter(|p| !all_children.contains(*p))
            .cloned()
            .collect();
        roots.sort(); // テストの安定性のため

        VirtualTree { children, is_file, roots, extra_dirs: HashMap::new() }
    }

    /// フォルダembedding行列（folders.bin）由来のディレクトリ一覧を追加登録する。
    ///
    /// `folder_paths` には親/兄弟探索の対象としたい全フォルダパスを渡す。
    /// 親パスごとにグルーピングし、`expand()` から兄弟ディレクトリとして参照される。
    pub fn with_extra_folders(mut self, folder_paths: &[PathBuf]) -> Self {
        let mut extra_dirs: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
        for dir in folder_paths {
            if let Some(parent) = dir.parent() {
                if parent.as_os_str().is_empty() {
                    continue;
                }
                extra_dirs.entry(parent.to_path_buf()).or_default().push(dir.clone());
            }
        }
        self.extra_dirs = extra_dirs;
        self
    }

    /// ノードの直接の子一覧を返す（なければ空スライス）
    pub fn children_of(&self, path: &Path) -> &[PathBuf] {
        self.children
            .get(path)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// ファイルか否か
    pub fn is_file(&self, path: &Path) -> bool {
        self.is_file.get(path).copied().unwrap_or(false)
    }

    /// Phase1候補（Everything結果由来の仮想ツリー）に含まれるノードか
    pub fn in_phase1(&self, path: &Path) -> bool {
        self.is_file.contains_key(path)
    }

    /// Phase1候補ファイルの総数（スコアリング対象件数）
    pub fn phase1_file_count(&self) -> usize {
        self.is_file.values().filter(|&&f| f).count()
    }

    /// ノードの展開先一覧を返す（AIサジェスト探索用）。
    ///
    /// Phase1候補ツリーの子（`children_of`）に加え、`with_extra_folders` で登録した
    /// 兄弟ディレクトリ、および未探索の親ディレクトリを候補として開放する。
    pub fn expand(&self, path: &Path) -> Vec<PathBuf> {
        let mut result: Vec<PathBuf> = self.children_of(path).to_vec();

        if let Some(parent) = path.parent() {
            // 兄弟ディレクトリ: folders.bin 由来の同階層フォルダを追加探索対象にする
            if let Some(siblings) = self.extra_dirs.get(parent) {
                for sibling in siblings {
                    if sibling != path && !result.contains(sibling) {
                        result.push(sibling.clone());
                    }
                }
            }

            // 親ディレクトリ: Phase1ツリー外（祖先として未登録）なら上方向にも開放する
            let parent = parent.to_path_buf();
            if !self.is_file.contains_key(&parent) && !result.contains(&parent) {
                result.push(parent);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pb(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn builds_tree_from_flat_paths() {
        let paths = vec![
            pb("/src/components/Button.tsx"),
            pb("/src/components/Input.tsx"),
            pb("/src/hooks/useButton.ts"),
        ];
        let tree = VirtualTree::from_paths(&paths);

        // ルートは /src のみ
        assert_eq!(tree.roots, vec![pb("/src")]);

        let mut src_children: Vec<_> = tree.children_of(Path::new("/src")).to_vec();
        src_children.sort();
        assert_eq!(
            src_children,
            vec![pb("/src/components"), pb("/src/hooks")]
        );

        assert!(!tree.is_file(Path::new("/src/components")));
        assert!(tree.is_file(Path::new("/src/components/Button.tsx")));
    }

    #[test]
    fn no_duplicate_children() {
        let paths = vec![
            pb("/root/dir/a.txt"),
            pb("/root/dir/b.txt"),
        ];
        let tree = VirtualTree::from_paths(&paths);
        // /root/dir の子は a.txt と b.txt の 2 件のみ（重複なし）
        assert_eq!(tree.children_of(Path::new("/root/dir")).len(), 2);
    }

    #[test]
    fn multiple_roots() {
        let paths = vec![
            pb("/a/x.txt"),
            pb("/b/y.txt"),
        ];
        let tree = VirtualTree::from_paths(&paths);
        assert_eq!(tree.roots.len(), 2);
    }

    #[test]
    fn in_phase1_reflects_candidate_paths() {
        let tree = VirtualTree::from_paths(&[pb("/src/components/Button.tsx")]);
        assert!(tree.in_phase1(Path::new("/src")));
        assert!(tree.in_phase1(Path::new("/src/components/Button.tsx")));
        assert!(!tree.in_phase1(Path::new("/src/utils")));
    }

    #[test]
    fn phase1_file_count_counts_only_files() {
        let tree = VirtualTree::from_paths(&[
            pb("/src/components/Button.tsx"),
            pb("/src/components/Input.tsx"),
            pb("/src/hooks/useButton.ts"),
        ]);
        assert_eq!(tree.phase1_file_count(), 3);
    }

    #[test]
    fn expand_includes_sibling_dirs_from_extra_folders() {
        let tree = VirtualTree::from_paths(&[pb("/src/components/Button.tsx")])
            .with_extra_folders(&[
                pb("/src/components"),
                pb("/src/utils"),
                pb("/src/hooks"),
            ]);

        let expanded = tree.expand(Path::new("/src/components"));
        assert!(expanded.contains(&pb("/src/utils")));
        assert!(expanded.contains(&pb("/src/hooks")));
        // 自分自身は含まない
        assert!(!expanded.contains(&pb("/src/components")));
        // Phase1の子は維持される
        assert!(expanded.contains(&pb("/src/components/Button.tsx")));
    }

    #[test]
    fn expand_opens_unvisited_parent_dir() {
        let tree = VirtualTree::from_paths(&[pb("/src/components/Button.tsx")])
            .with_extra_folders(&[pb("/other/deep")]);

        // /other/deep の親 /other はPhase1ツリーに含まれないため expand で開放される
        let expanded = tree.expand(Path::new("/other/deep"));
        assert!(expanded.contains(&pb("/other")));
    }

    #[test]
    fn expand_without_extra_folders_matches_children_of() {
        let tree = VirtualTree::from_paths(&[pb("/src/components/Button.tsx")]);
        assert_eq!(tree.expand(Path::new("/src/components")), tree.children_of(Path::new("/src/components")).to_vec());
    }
}
