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

        VirtualTree { children, is_file, roots }
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
}
