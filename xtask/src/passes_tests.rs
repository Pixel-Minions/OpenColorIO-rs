// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;

fn names(list: &[&str]) -> Tested {
    Tested::Packages(list.iter().map(|n| n.to_string()).collect())
}

/// A full pass of tree `t1` in release and in Rocky Linux 9, of the whole workspace.
fn full_pass() -> Record {
    Record {
        tree: "t1".into(),
        commit: "c1".into(),
        time: "20261122T051209Z".into(),
        log_dir: "D:/w/target/gate-logs/20261122T051209Z".into(),
        full: true,
        release: true,
        rocky: true,
        main: false,
        tested: Tested::Workspace,
        rustc: "rustc 1.98.1 (abc 2026-10-01)".into(),
        machine: "windows-x86_64 cpu=X cpuid=GenuineIntel:000906ea features=sse2,avx2".into(),
    }
}

/// What a land of tree `t1` with Rocky Linux 9 needs, on the same compiler and machine.
fn need(tested: Tested) -> Need {
    let pass = full_pass();
    Need {
        tree: "t1".into(),
        tested,
        rocky: true,
        rustc: pass.rustc,
        machine: pass.machine,
    }
}

#[test]
fn records_round_trip() {
    let mut quick = full_pass();
    quick.full = false;
    quick.release = false;
    quick.rocky = false;
    quick.main = true;
    quick.tested = names(&["xtask", "opencolorio-rs-ops"]);
    let mut none = full_pass();
    none.tested = Tested::Nothing;
    let records = vec![full_pass(), quick, none];
    let text = render(&records);
    assert_eq!(parse(&text).unwrap(), records);
    assert!(text.contains("[[pass]]"), "{text}");
    assert!(text.contains("tier = \"quick\""), "{text}");
    assert!(text.contains("tested = \"workspace\""), "{text}");
    assert!(text.contains("tested = \"nothing\""), "{text}");
    assert_eq!(parse("").unwrap(), Vec::new());
    assert!(parse("pass = 1").is_err());
    assert!(parse(&text.replace("tier = \"quick\"", "tier = \"slow\"")).is_err());
    assert!(parse(&text.replace("rustc = ", "rust = ")).is_err());
}

#[test]
fn what_tests_cover() {
    let ws = Tested::Workspace;
    let none = Tested::Nothing;
    assert!(ws.covers(&ws) && ws.covers(&names(&["a"])) && ws.covers(&none));
    assert!(none.covers(&none));
    assert!(!none.covers(&ws) && !none.covers(&names(&["a"])));
    assert!(names(&["a", "b"]).covers(&names(&["b"])));
    assert!(names(&["a", "b"]).covers(&names(&["b", "a"])));
    assert!(names(&["a"]).covers(&none));
    assert!(!names(&["a"]).covers(&names(&["a", "b"])));
    assert!(!names(&["a", "b"]).covers(&ws));
}

#[test]
fn a_record_covers_a_land_only_when_everything_matches() {
    let pass = full_pass();
    assert_eq!(not_covered(&pass, &need(Tested::Workspace)), None);
    assert_eq!(not_covered(&pass, &need(names(&["xtask"]))), None);
    let mut land = need(Tested::Workspace);
    land.rocky = false;
    assert_eq!(not_covered(&pass, &land), None);
    let mut no_rocky = pass.clone();
    no_rocky.rocky = false;
    assert_eq!(not_covered(&no_rocky, &land), None);

    let cases: Vec<(Record, &str)> = vec![
        (
            Record {
                tree: "t2".into(),
                ..pass.clone()
            },
            "another tree",
        ),
        (
            Record {
                full: false,
                ..pass.clone()
            },
            "the quick tier",
        ),
        (
            Record {
                release: false,
                ..pass.clone()
            },
            "no release tests",
        ),
        (no_rocky, "no Rocky Linux 9 pass"),
        (
            Record {
                rustc: "rustc 1.99.0 (def 2026-11-01)".into(),
                ..pass.clone()
            },
            "another rustc",
        ),
        (
            Record {
                machine: pass.machine.replace("avx2", "avx512f"),
                ..pass.clone()
            },
            "another machine or CPU",
        ),
        (
            Record {
                tested: names(&["xtask"]),
                ..pass.clone()
            },
            "fewer packages",
        ),
        (
            Record {
                tested: Tested::Nothing,
                ..pass.clone()
            },
            "fewer packages",
        ),
    ];
    for (record, why) in &cases {
        assert_eq!(not_covered(record, &need(Tested::Workspace)), Some(*why));
    }
    // `--main` isn't needed.
    assert!(!pass.main);
}

#[test]
fn the_newest_covering_record_wins() {
    let mut older = full_pass();
    older.time = "20261121T000000Z".into();
    let mut quick = full_pass();
    quick.full = false;
    let records = vec![older.clone(), full_pass(), quick.clone()];
    assert_eq!(
        covering(&records, &need(Tested::Workspace)).unwrap(),
        &records[1]
    );
    let found = covering(&[older.clone(), quick.clone()], &need(names(&["x"])))
        .map(|r| r.time.clone())
        .unwrap();
    assert_eq!(found, older.time);
    let none = covering(std::slice::from_ref(&quick), &need(Tested::Workspace)).unwrap_err();
    assert!(none.contains("1 recorded pass(es)"), "{none}");
    assert!(none.contains("the quick tier"), "{none}");
    let empty = covering(&[], &need(Tested::Workspace)).unwrap_err();
    assert_eq!(empty, "no recorded pass of this tree");
}

#[test]
fn trees_equal_apart_from_the_generated_files() {
    assert!(only_generated(&[]));
    assert!(only_generated(&["docs/parity.md"]));
    assert!(only_generated(&["docs/ratchet.toml", "docs/parity.md"]));
    assert!(!only_generated(&["docs/parity.md", "xtask/src/land.rs"]));
    assert!(!only_generated(&["docs/improvements.md"]));
    assert!(!only_generated(&["docs/parity.mdx"]));
    assert!(!only_generated(&["parity.md"]));
}

#[test]
fn only_a_clean_tree_has_a_state() {
    let state = clean_state("", "tree\n", "commit\n").unwrap();
    assert_eq!(state.tree, "tree");
    assert_eq!(state.commit, "commit");
    assert_eq!(
        clean_state("\n", "t", "c").map(|s| s.tree),
        Some("t".into())
    );
    assert_eq!(clean_state(" M xtask/src/gate.rs\n", "t", "c"), None);
    assert_eq!(clean_state("?? notes.txt\n", "t", "c"), None);
    assert_eq!(clean_state("A  new.rs\n", "t", "c"), None);
}

/// A scratch repository with one commit, and an empty records directory beside it.
struct Scratch {
    repo: crate::register_merge::testing::Repo,
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Scratch {
        let repo = crate::register_merge::testing::Repo::new(&format!("passes-{name}"), "union");
        repo.commit("# Register\n", "first");
        let dir = repo.path().with_extension("passes");
        let _ = std::fs::remove_dir_all(&dir);
        Scratch { repo, dir }
    }

    fn fill(state: &TreeState) -> Result<Record, String> {
        Ok(Record {
            tree: state.tree.clone(),
            commit: state.commit.clone(),
            ..full_pass()
        })
    }

    fn files(&self) -> usize {
        std::fs::read_dir(&self.dir).map_or(0, |d| {
            d.flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "toml"))
                .count()
        })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn a_clean_tree_is_recorded_and_read_back() {
    let s = Scratch::new("clean");
    let root = s.repo.path();
    let start = tree_state(root).unwrap().unwrap();
    let head = s.repo.git(&["rev-parse", "HEAD^{tree}"]).unwrap();
    assert_eq!(start.tree, head.trim());
    let written = record(root, &s.dir, Some(&start), Scratch::fill).unwrap();
    assert_eq!(
        written,
        Recorded::Written(s.dir.join(format!("{}.toml", start.tree)))
    );
    let records = read(&s.dir, &start.tree).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].tree, start.tree);
    assert_eq!(records[0].commit, start.commit);
    // A second pass of the same tree is appended.
    record(root, &s.dir, Some(&start), Scratch::fill).unwrap();
    assert_eq!(read(&s.dir, &start.tree).unwrap().len(), 2);
    assert_eq!(s.files(), 1);
    assert_eq!(read(&s.dir, "another").unwrap(), Vec::new());
}

#[test]
fn a_dirty_tree_writes_no_record() {
    let s = Scratch::new("dirty");
    let root = s.repo.path();
    let clean = tree_state(root).unwrap().unwrap();
    std::fs::write(root.join("notes.txt"), "untracked\n").unwrap();
    // Dirty from the start: no state, and nothing written.
    assert_eq!(tree_state(root).unwrap(), None);
    let mut filled = false;
    let none = record(root, &s.dir, None, |state| {
        filled = true;
        Scratch::fill(state)
    })
    .unwrap();
    assert_eq!(none, Recorded::Dirty);
    assert!(!filled);
    // Clean at the start, dirty at the end.
    assert_eq!(
        record(root, &s.dir, Some(&clean), Scratch::fill).unwrap(),
        Recorded::Dirty
    );
    // A change to a tracked file.
    std::fs::remove_file(root.join("notes.txt")).unwrap();
    std::fs::write(root.join("docs/improvements.md"), "# Changed\n").unwrap();
    assert_eq!(
        record(root, &s.dir, Some(&clean), Scratch::fill).unwrap(),
        Recorded::Dirty
    );
    assert_eq!(s.files(), 0);
    assert!(!s.dir.exists());
}

#[test]
fn a_head_that_moves_writes_no_record() {
    let s = Scratch::new("moved");
    let root = s.repo.path();
    let start = tree_state(root).unwrap().unwrap();
    s.repo.commit("# Register\n\nMore.\n", "second");
    assert_eq!(
        record(root, &s.dir, Some(&start), Scratch::fill).unwrap(),
        Recorded::Moved
    );
    assert_eq!(s.files(), 0);
}
