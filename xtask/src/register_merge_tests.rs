// SPDX-License-Identifier: BSD-3-Clause
// Copyright Contributors to the OpenColorIO Project.

use super::*;

/// A register: a title and its preamble, two sections, and the undefined-behaviour section.
const BASE: &str = "\
# Improvement candidates

Preamble.

## Ops

### I-1. One

- **Upstream:** one.
- **Status:** open.

### I-18. Two

- **Status:** matched.

## Undefined behaviour upstream

Intro of the section.

### U-31. Thirty-one

- **Status:** matched in 2.3b.

### U-46. Forty-six

- **Status:** p3-context.
  stray line one
  stray line two
";

fn clean(merged: Merged) -> String {
    match merged {
        Merged::Clean(text) => text,
        Merged::Conflict { conflicts, text } => panic!("conflicts {conflicts:?}:\n{text}"),
    }
}

#[test]
fn a_register_in_its_form_reads_back_exactly() {
    assert_eq!(Register::parse(BASE).unwrap().render(), BASE);
    let root = crate::root();
    let real = std::fs::read_to_string(root.join(REGISTER)).unwrap();
    assert_eq!(Register::parse(&real).unwrap().render(), real);
}

#[test]
fn a_register_not_in_its_form_is_refused() {
    for (text, fragment) in [
        (BASE.replace('\n', "\r\n"), "carriage returns"),
        (BASE.replace("\n\n### I-18.", "\n### I-18."), "form"),
        (
            BASE.replace("open.\n\n### I-18.", "open.\n\n\n### I-18."),
            "form",
        ),
        (format!("{BASE}\n"), "form"),
        (BASE.replace("### I-18. Two", "### I-1. Two"), "twice"),
        (
            BASE.replace("### I-18. Two", "### Two"),
            "not an entry heading",
        ),
    ] {
        let error = Register::parse(&text).unwrap_err();
        assert!(error.contains(fragment), "{error}");
    }
}

#[test]
fn entries_added_on_both_sides_of_one_section_are_both_kept_in_order() {
    let ours = BASE.replace(
        "### I-18. Two",
        "### I-5. Ours\n\n- **Status:** ours.\n\n### I-18. Two",
    );
    let theirs = BASE.replace(
        "## Undefined behaviour upstream",
        "### I-20. Theirs\n\n- **Status:** theirs.\n\n## Undefined behaviour upstream",
    );
    let merged = clean(merge3(BASE, &ours, &theirs).unwrap());
    let expected = ours.replace(
        "## Undefined behaviour upstream",
        "### I-20. Theirs\n\n- **Status:** theirs.\n\n## Undefined behaviour upstream",
    );
    assert_eq!(merged, expected);
    // The same entry number added on both sides with different text is a conflict.
    let other = BASE.replace(
        "### I-18. Two",
        "### I-5. Other\n\n- **Status:** other.\n\n### I-18. Two",
    );
    assert!(matches!(
        merge3(BASE, &ours, &other).unwrap(),
        Merged::Conflict { .. }
    ));
}

#[test]
fn edits_to_different_entries_merge() {
    let ours = BASE.replace("- **Upstream:** one.", "- **Upstream:** one, ours.");
    let theirs = BASE.replace("matched in 2.3b.", "matched in 2.3b and 2.3d1.");
    let merged = clean(merge3(BASE, &ours, &theirs).unwrap());
    assert_eq!(
        merged,
        ours.replace("matched in 2.3b.", "matched in 2.3b and 2.3d1.")
    );
    // Both sides making the same change is no conflict.
    assert_eq!(clean(merge3(BASE, &theirs, &theirs).unwrap()), theirs);
}

#[test]
fn a_removal_whose_context_moved_still_applies() {
    // Ours appended U-52 after U-46's stray lines; theirs removes the stray lines. A text merge
    // of the two keeps them (they are next to the append); entry by entry, U-46 takes theirs.
    let ours = format!("{BASE}\n### U-52. Fifty-two\n\n- **Status:** to port.\n");
    let theirs = BASE.replace("  stray line one\n  stray line two\n", "");
    let merged = clean(merge3(BASE, &ours, &theirs).unwrap());
    assert_eq!(
        merged,
        format!("{theirs}\n### U-52. Fifty-two\n\n- **Status:** to port.\n")
    );
}

#[test]
fn an_entry_changed_differently_on_both_sides_is_a_conflict_with_markers() {
    let ours = BASE.replace("matched in 2.3b.", "matched in 2.3b, ours.");
    let theirs = BASE.replace("matched in 2.3b.", "matched in 2.3b, theirs.");
    let Merged::Conflict { text, conflicts } = merge3(BASE, &ours, &theirs).unwrap() else {
        panic!("no conflict");
    };
    assert_eq!(conflicts, ["U-31 changed on both sides"]);
    assert!(text.contains(
        "<<<<<<< ours\n### U-31. Thirty-one\n\n- **Status:** matched in 2.3b, ours.\n=======\n\
         ### U-31. Thirty-one\n\n- **Status:** matched in 2.3b, theirs.\n>>>>>>> theirs\n"
    ));
    // Removed on one side and edited on the other.
    let removed = BASE.replace(
        "### U-31. Thirty-one\n\n- **Status:** matched in 2.3b.\n\n",
        "",
    );
    assert!(matches!(
        merge3(BASE, &ours, &removed).unwrap(),
        Merged::Conflict { .. }
    ));
    // The same section intro changed differently.
    let a = BASE.replace("Intro of the section.", "Intro, ours.");
    let b = BASE.replace("Intro of the section.", "Intro, theirs.");
    let Merged::Conflict { conflicts, .. } = merge3(BASE, &a, &b).unwrap() else {
        panic!("no conflict");
    };
    assert_eq!(conflicts, ["the text of `## Undefined behaviour upstream`"]);
}

#[test]
fn removed_moved_and_new_sections_follow_theirs() {
    // Theirs removes I-1, moves I-18 to a new section after `## Ops`, and edits the preamble.
    let theirs = BASE
        .replace(
            "### I-1. One\n\n- **Upstream:** one.\n- **Status:** open.\n\n",
            "",
        )
        .replace(
            "### I-18. Two\n\n- **Status:** matched.\n\n",
            "## New\n\n### I-18. Two\n\n- **Status:** matched.\n\n",
        )
        .replace("Preamble.", "Preamble, theirs.");
    // Ours adds U-40 between U-31 and U-46.
    let ours = BASE.replace(
        "### U-46. Forty-six",
        "### U-40. Forty\n\n- **Status:** ours.\n\n### U-46. Forty-six",
    );
    let merged = clean(merge3(BASE, &ours, &theirs).unwrap());
    let expected = theirs.replace(
        "### U-46. Forty-six",
        "### U-40. Forty\n\n- **Status:** ours.\n\n### U-46. Forty-six",
    );
    assert_eq!(merged, expected);
}

#[test]
fn the_driver_writes_the_merge_into_ours() {
    let dir = std::env::temp_dir().join(format!("ocio-rs-xtask-register-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (b, o, t) = (dir.join("base"), dir.join("ours"), dir.join("theirs"));
    let ours = format!("{BASE}\n### U-52. Fifty-two\n\n- **Status:** to port.\n");
    let theirs = BASE.replace("  stray line one\n  stray line two\n", "");
    std::fs::write(&b, BASE).unwrap();
    std::fs::write(&o, &ours).unwrap();
    std::fs::write(&t, &theirs).unwrap();
    driver(&b, &o, &t).unwrap();
    assert_eq!(
        std::fs::read_to_string(&o).unwrap(),
        format!("{theirs}\n### U-52. Fifty-two\n\n- **Status:** to port.\n")
    );
    // A conflict: an error, and the markers in ours.
    std::fs::write(&o, BASE.replace("matched in 2.3b.", "ours.")).unwrap();
    std::fs::write(&t, BASE.replace("matched in 2.3b.", "theirs.")).unwrap();
    let error = driver(&b, &o, &t).unwrap_err();
    assert!(error.contains("U-31 changed on both sides"), "{error}");
    assert!(
        std::fs::read_to_string(&o)
            .unwrap()
            .contains("<<<<<<< ours")
    );
    // Not a register in its form: an error, ours untouched.
    std::fs::write(&o, "not a register").unwrap();
    assert!(driver(&b, &o, &t).is_err());
    assert_eq!(std::fs::read_to_string(&o).unwrap(), "not a register");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn differences_name_the_entries() {
    let other = BASE.replace("matched in 2.3b.", "x.");
    assert_eq!(differences(BASE, &other), ["U-31"]);
    assert!(differences(BASE, BASE).is_empty());
}

/// The merge driver of the end-to-end tests below (`testing::Repo`), not a test of its own:
/// git runs this test binary with `OCIO_RS_TEST_MERGE_DRIVER` naming the driver's three files
/// (`%O|%A|%B`) and this test alone, which then does what `cargo xtask merge-register %O %A %B`
/// does: its exit status is the driver's. Without the variable it has nothing to do.
#[test]
fn merge_driver_of_the_end_to_end_tests() {
    let Ok(files) = std::env::var("OCIO_RS_TEST_MERGE_DRIVER") else {
        return;
    };
    let files: Vec<&str> = files.split('|').collect();
    let [base, ours, theirs] = files[..] else {
        panic!("OCIO_RS_TEST_MERGE_DRIVER: {files:?}");
    };
    if let Err(e) = driver(Path::new(base), Path::new(ours), Path::new(theirs)) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// End to end, as `xtask land` replays and as agents cherry-pick: the removal after which
/// phase0 appended an entry (lost by git's text merge on 2026-10-06) survives a rebase and a
/// cherry-pick.
#[test]
fn a_removal_after_which_phase0_appended_survives_the_replay() {
    use testing::{STRAY, appended, fixed};
    let repo = testing::Repo::new("removal", "ocio-register");
    repo.commit(STRAY, "base");
    repo.git(&["checkout", "--quiet", "-b", "card"]).unwrap();
    let card = repo.commit(&fixed(), "card: remove the stray lines");
    repo.git(&["checkout", "--quiet", "main"]).unwrap();
    repo.commit(&appended(STRAY), "phase0: append U-52");
    repo.git(&["checkout", "--quiet", "-b", "replay", &card])
        .unwrap();
    repo.git(&["rebase", "--quiet", "main"]).unwrap();
    assert_eq!(repo.register(), appended(&fixed()));
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]).unwrap(),
        "card: remove the stray lines"
    );
    repo.git(&["checkout", "--quiet", "-b", "picked", "main"])
        .unwrap();
    repo.git(&["cherry-pick", &card]).unwrap();
    assert_eq!(repo.register(), appended(&fixed()));
}

/// End to end: entries appended at the end of the file on both sides are both kept, in order.
#[test]
fn entries_appended_on_both_sides_are_both_kept() {
    use testing::STRAY;
    let repo = testing::Repo::new("append", "ocio-register");
    repo.commit(STRAY, "base");
    let entry = |id: &str| format!("\n### {id}. New\n\n- **Status:** {id}.\n");
    repo.git(&["checkout", "--quiet", "-b", "card"]).unwrap();
    let card = repo.commit(&format!("{STRAY}{}", entry("U-53")), "card: U-53");
    repo.git(&["checkout", "--quiet", "main"]).unwrap();
    repo.commit(&format!("{STRAY}{}", entry("U-52")), "phase0: U-52");
    repo.git(&["cherry-pick", &card]).unwrap();
    assert_eq!(
        repo.register(),
        format!("{STRAY}{}{}", entry("U-52"), entry("U-53"))
    );
}

/// End to end: an entry changed differently on both sides stops the cherry-pick, with both
/// versions between conflict markers in the file.
#[test]
fn an_entry_changed_on_both_sides_stops_the_cherry_pick() {
    use testing::STRAY;
    let repo = testing::Repo::new("conflict", "ocio-register");
    repo.commit(STRAY, "base");
    repo.git(&["checkout", "--quiet", "-b", "card"]).unwrap();
    let card = repo.commit(
        &STRAY.replace("and in 2.3c1.", "and in 2.3c1, card."),
        "card: U-31",
    );
    repo.git(&["checkout", "--quiet", "main"]).unwrap();
    repo.commit(
        &STRAY.replace("and in 2.3c1.", "and in 2.3c1, phase0."),
        "phase0: U-31",
    );
    let error = repo.git(&["cherry-pick", &card]).unwrap_err();
    assert!(error.contains("U-31 changed on both sides"), "{error}");
    let text = repo.register();
    assert!(text.contains("<<<<<<< ours"), "{text}");
    assert!(
        text.contains("2.3c1, phase0.") && text.contains("2.3c1, card."),
        "{text}"
    );
    repo.git(&["cherry-pick", "--abort"]).unwrap();
}
